mod socket_manager;
mod opus_player;
mod oeggs;
mod http_server;
mod ws_server;

use std::{env::{self}, path::Path, sync::Arc};
use tokio::{fs, io::{self, AsyncBufReadExt, BufReader}};

use crate::{http_server::{HTTPServerContext, init_http_server}, opus_player::OpusPlayerHandle, ws_server::{WSServerContext, init_ws_server}};

#[tokio::main]
async fn main() -> io::Result<()> {
    let http_port: u16 = env::var("HTTP_PORT")
        .expect("Should specify a HTTP_PORT env variable").parse()
        .expect("PORT should be a number");
    let ws_port: u16 = env::var("WS_PORT")
        .expect("Should specify a WS_PORT env variable").parse()
        .expect("PORT should be a number");

    let ogg_player = OpusPlayerHandle::new();

    let http_server_player = ogg_player.clone();
    let http_server_handle = tokio::spawn(async move {
        let ctx = HTTPServerContext {
            player: http_server_player,
        };

        init_http_server(http_port, ctx).await.expect("Should start http server");
    });

    let ws_server_player = ogg_player.clone();
    let ws_server_handle = tokio::spawn(async move {
        let ctx = WSServerContext {
            player: ws_server_player,
        };

        init_ws_server(ws_port, ctx).await.expect("Should start WS server")
    });


    let cli_player = ogg_player.clone();
    let cli_handle = tokio::spawn(async move {
        loop {
            let playern = cli_player.clone();
            let mut input = String::new();
            let mut reader = BufReader::new(io::stdin());
            match reader.read_line(&mut input).await {
                Ok(n) => n,
                Err(e) => {
                    println!("Error reading stdin: {}", e);
                    break;
                }
            };

            match play_playlist(playern, input.trim().to_string()).await {
                Ok(_) => println!("Started playing playlist: {}", input.trim()),
                Err(e) => println!("Error starting playlist {}: {}", input.trim(), e),
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

async fn play_playlist(player: OpusPlayerHandle, path: String) -> Result<(), String> {
    let files = get_playlist_files(&path).await?;
    
    let count = files.len();
    let mut i = 0;

    println!("Spawning player");
    tokio::spawn(async move {
        loop {
            let file = &files[i % count];

            match player.play_file(file.clone()).await {
                Ok(_) => println!("Finished playing file: {}", file),
                Err(e) => println!("Error playing file {}: {}", file, e),
            };

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
        return Err("No .ogg files found in the specified directory".to_string());
    }

    file_names.sort_by(|a, b| a.cmp(b));

    Ok(file_names)
}
