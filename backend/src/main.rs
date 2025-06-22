mod socket_manager;
mod opus_player;
mod oeggs;
mod http_server;
mod ws_server;

use std::{env::{self, join_paths}, io::{self, Write}, path::Path, sync::Arc};
use tokio::fs;

use opus_player::OpusPlayer;

use crate::{http_server::{init_http_server, HTTPServerContext}, ws_server::{init_ws_server, WSServerContext}};

#[tokio::main]
async fn main() -> io::Result<()> {
    let http_port: u16 = env::var("HTTP_PORT")
        .expect("Should specify a HTTP_PORT env variable").parse()
        .expect("PORT should be a number");
    let ws_port: u16 = env::var("WS_PORT")
        .expect("Should specify a WS_PORT env variable").parse()
        .expect("PORT should be a number");

    let ogg_player = Arc::from(OpusPlayer::new());

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


    let player = ogg_player.clone();

    let player_handle = tokio::spawn(async move {

        loop {
            let mut input = String::new();
            io::stdin().read_line(&mut input).expect("Failed to read line");
            let playern = player.clone();

            let files = get_playlist_files(&input.trim()).await;
            
            let count = files.len();
            let mut i = 0;

            tokio::spawn(async move {
                let playlist_player = playern.clone();

                loop {
                    let file = &files[i % count];

                    playlist_player.play_file(file).await.expect("Should play file");

                    i += 1;
                }
            });
        }
    });

    let _ = tokio::join!(
        http_server_handle,
        ws_server_handle,
        player_handle,
    );

    Ok(())
}


async fn get_playlist_files(path: &str) -> Vec<String> {
    let mut dir = fs::read_dir(path).await.expect("Playlist folder should be accessible");
    let mut file_names = Vec::new();

    while let Some(entry) = dir.next_entry().await.expect("Should get next file") {
        let metadata = entry.metadata().await.expect("Should get file metadata");
        if metadata.is_file() {
            if let Some(name) = entry.file_name().to_str() {
                let full_path = Path::new(&path).join(name);
                file_names.push(full_path.to_string_lossy().to_string());
            }
        }
    }

    file_names.sort_by(|a, b| a.cmp(b));

    file_names
}
