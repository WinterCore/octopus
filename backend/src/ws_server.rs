use std::{net::SocketAddr, sync::Arc};

use futures_util::{future, SinkExt, StreamExt, TryStreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::{stream, Message, Utf8Bytes};

use crate::opus_player::OpusPlayer;


pub struct WSServerContext {
    pub player: Arc<OpusPlayer>,
}

pub async fn init_ws_server(
    port: u16,
    ctx: WSServerContext,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");

    let ctx_arc = Arc::from(ctx);

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(accept_connection(ctx_arc.clone(), stream));
    }

    Ok(())
}

async fn accept_connection(
    ctx: Arc<WSServerContext>,
    stream: TcpStream,
) -> Result<(), String> {
    let addr = stream.peer_addr().expect("connected streams should have a peer address");

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");


    let (mut write, mut read) = ws_stream.split();
    
    let read_ctx = ctx.clone();
    tokio::spawn(async move {
        while let Some(msg_result) = read.next().await {
            if let Ok(text) = msg_result {
                if text.into_text().unwrap() == "metadata" {
                    let metadata = read_ctx.player.get_metadata().await;

                    let json = format!(
                        r#"{{
                            "name": "{}",
                            "author": "{}",
                            "active_file_start_time_ms": {},
                            "active_file_duration_ms": {},
                            "image": {}
                        }}"#,
                        metadata.name,
                        metadata.author,
                        read_ctx.player.get_current_file_start_time_ms().await,
                        metadata.duration_ms,
                        if let Some(url) = metadata.image { format!("\"{}\"", url) } else { "null".to_string() },
                    );

                    write.send(Message::Text(Utf8Bytes::from(json))).await.expect("Should send player info");
                }
            }
        }
    });
    
    Ok(())
}
