mod socket_manager;
mod opus_player;
mod oeggs;
mod http_server;
mod ws_server;
mod config;
mod auth;

use std::{collections::HashMap, env, path::{Path, PathBuf}, sync::Arc};
use tokio::{fs, io::{self, AsyncBufReadExt, BufReader}, sync::{broadcast, RwLock}};

use crate::{
    auth::AuthState,
    config::{StreamConfig, StreamsConfig},
    http_server::{HTTPServerContext, init_http_server},
    opus_player::{OpusPlayerHandle, PlaybackResult},
    ws_server::{WSServerContext, init_ws_server, get_metadata_json},
};

pub struct StreamEntry {
    pub config: StreamConfig,
    pub player: OpusPlayerHandle,
    pub metadata_tx: broadcast::Sender<String>,
}

pub type StreamRegistry = Arc<RwLock<HashMap<String, Arc<RwLock<StreamEntry>>>>>;

pub struct AppState {
    pub registry: StreamRegistry,
    pub default_stream: Option<String>,
    pub config_path: PathBuf,
    pub auth: Arc<AuthState>,
}

fn parse_config_arg() -> PathBuf {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
            panic!("--config requires a path");
        }
        if let Some(rest) = arg.strip_prefix("--config=") {
            return PathBuf::from(rest);
        }
    }
    panic!("Missing --config <path> argument");
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let http_port: u16 = env::var("HTTP_PORT")
        .expect("Should specify a HTTP_PORT env variable").parse()
        .expect("PORT should be a number");
    let ws_port: u16 = env::var("WS_PORT")
        .expect("Should specify a WS_PORT env variable").parse()
        .expect("PORT should be a number");

    let admin_password = env::var("ADMIN_PASSWORD")
        .expect("Should specify an ADMIN_PASSWORD env variable");

    let config_path = parse_config_arg();
    let streams_config = StreamsConfig::load(&config_path)
        .unwrap_or_else(|e| panic!("Failed to load config from {:?}: {}", config_path, e));

    let registry: StreamRegistry = Arc::new(RwLock::new(HashMap::new()));

    // Spawn one player + playlist task per configured stream.
    for stream_cfg in &streams_config.streams {
        let player = OpusPlayerHandle::new();
        let (metadata_tx, _) = broadcast::channel::<String>(100);

        let entry = StreamEntry {
            config: stream_cfg.clone(),
            player: player.clone(),
            metadata_tx: metadata_tx.clone(),
        };
        registry.write().await.insert(stream_cfg.id.clone(), Arc::new(RwLock::new(entry)));

        let playlist_path = stream_cfg.playlist.clone();
        let stream_id = stream_cfg.id.clone();
        let stream_name = stream_cfg.name.clone();
        let registry_for_task = registry.clone();
        tokio::spawn(async move {
            if let Err(e) = play_playlist(
                player,
                metadata_tx,
                playlist_path.clone(),
                stream_id.clone(),
                stream_name,
                registry_for_task,
            ).await {
                eprintln!("Stream '{}' failed to start playlist '{}': {}", stream_id, playlist_path, e);
            }
        });
    }

    let app_state = Arc::new(AppState {
        registry: registry.clone(),
        default_stream: streams_config.default_stream.clone(),
        config_path: config_path.clone(),
        auth: Arc::new(AuthState::new(admin_password)),
    });

    let http_state = app_state.clone();
    let http_server_handle = tokio::spawn(async move {
        let ctx = HTTPServerContext { app: http_state };
        init_http_server(http_port, ctx).await.expect("Should start http server");
    });

    let ws_state = app_state.clone();
    let ws_server_handle = tokio::spawn(async move {
        let ctx = WSServerContext { app: ws_state };
        init_ws_server(ws_port, ctx).await.expect("Should start WS server")
    });

    let fifo_path = env::var("CONTROL_PIPE").unwrap_or_else(|_| "./control.fifo".to_string());

    if !Path::new(&fifo_path).exists() {
        std::process::Command::new("mkfifo")
            .arg(&fifo_path)
            .status()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to create control FIFO: {}", e)))?;
    }

    let cli_state = app_state.clone();
    let cli_handle = tokio::spawn(async move {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true) // O_RDWR: prevents blocking on open and avoids EOF
            .open(&fifo_path)
            .await
            .expect("Should open control FIFO");

        let mut reader = BufReader::new(file);

        loop {
            let mut input = String::new();
            match reader.read_line(&mut input).await {
                Ok(_) => {},
                Err(e) => {
                    println!("Error reading control FIFO: {}", e);
                    break;
                }
            };

            let line = input.trim();
            if line.is_empty() {
                continue;
            }

            // CONTROL_PIPE writes always target the default stream.
            let default_stream_id = match &cli_state.default_stream {
                Some(id) => id.clone(),
                None => {
                    println!("Control FIFO write ignored: no default_stream configured");
                    continue;
                }
            };
            let entry = match cli_state.registry.read().await.get(&default_stream_id).cloned() {
                Some(e) => e,
                None => {
                    println!("Default stream '{}' not found in registry", default_stream_id);
                    continue;
                }
            };
            let (player, metadata_tx, stream_name) = {
                let e = entry.read().await;
                (e.player.clone(), e.metadata_tx.clone(), e.config.name.clone())
            };

            match play_playlist(
                player,
                metadata_tx,
                line.to_string(),
                default_stream_id.clone(),
                stream_name,
                cli_state.registry.clone(),
            ).await {
                Ok(_) => println!("Started playing playlist on default stream: {}", line),
                Err(e) => println!("Error starting playlist {}: {}", line, e),
            };
        }
    });

    let _ = tokio::join!(
        http_server_handle,
        ws_server_handle,
        cli_handle,
    );

    Ok(())
}

async fn play_playlist(
    player: OpusPlayerHandle,
    metadata_tx: tokio::sync::broadcast::Sender<String>,
    path: String,
    stream_id: String,
    stream_name_at_start: String,
    registry: StreamRegistry,
) -> Result<(), String> {
    let files = get_playlist_files(&path).await?;

    let count = files.len();
    let mut i = 0;

    println!("Spawning player for stream '{}' playlist: {}", stream_id, path);
    tokio::spawn(async move {
        loop {
            let file = &files[i % count];

            // Look up the current stream name from the registry so renames are
            // reflected in broadcast metadata. Falls back to the initial name
            // if the entry has gone away.
            let stream_name = match registry.read().await.get(&stream_id).cloned() {
                Some(entry) => entry.read().await.config.name.clone(),
                None => stream_name_at_start.clone(),
            };

            let handles = match player.play_file(file.clone()).await {
                Ok(h) => h,
                Err(e) => {
                    println!("Error issuing play_file for {}: {}", file, e);
                    i += 1;
                    continue;
                }
            };

            // Wait for the actor to confirm the new file is now the active one,
            // then broadcast metadata so connected listeners see the new track
            // (covers normal playlist advance, skip, and any other trigger).
            if handles.started.await.is_ok() {
                if let Ok(json) = get_metadata_json(&player, Some(&stream_name), Some(&stream_id)).await {
                    let _ = metadata_tx.send(json);
                }
            }

            let result = match handles.result.await {
                Ok(r) => r,
                Err(e) => {
                    println!("Lost play_file result channel for {}: {}", file, e);
                    i += 1;
                    continue;
                }
            };

            match result {
                PlaybackResult::Finished => {
                    println!("Finished playback normally for file: {}", file);
                },
                PlaybackResult::Skipped => {
                    println!("Playback was skipped for file: {}", file);
                },
                PlaybackResult::Interrupted => {
                    println!("Playback was interrupted for file: {}", file);
                    return;
                },
                PlaybackResult::Error(e) => println!("Error during playback of file {}: {}", file, e),
            }

            i += 1;
        }
    });

    Ok(())
}

async fn get_playlist_files(path: &str) -> Result<Vec<String>, String> {
    let mut dir = fs::read_dir(path).await.map_err(|x| x.to_string())?;
    let mut file_names = Vec::new();

    println!("Loading playlist folder: {}", path);
    while let Some(entry) = dir.next_entry().await.map_err(|x| x.to_string())? {
        let metadata = entry.metadata().await.map_err(|x| x.to_string())?;
        if metadata.is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("opus") {
            if let Some(name) = entry.file_name().to_str() {
                let full_path = Path::new(&path).join(name);
                file_names.push(full_path.to_string_lossy().to_string());
            }
        }
    }

    if file_names.is_empty() {
        return Err("No .opus files found in the specified directory".to_string());
    }

    file_names.sort_by(|a, b| a.cmp(b));

    Ok(file_names)
}
