use std::{net::SocketAddr, sync::Arc};

use futures_util::{future, SinkExt, StreamExt, TryStreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::{stream, Message, Utf8Bytes};

use crate::opus_player::{BUFFER_SIZE_MS, OpusPlayer, OpusPlayerHandle, TimeData};


pub struct WSServerContext {
    pub player: OpusPlayerHandle,
    pub metadata_broadcast: tokio::sync::broadcast::Sender<String>,
}

pub async fn init_ws_server(
    port: u16,
    ctx: WSServerContext,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
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

    // Create a channel for sending messages to this client
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Subscribe to metadata broadcasts
    let mut metadata_rx = ctx.metadata_broadcast.subscribe();
    let broadcast_tx = tx.clone();
    tokio::spawn(async move {
        while let Ok(metadata_json) = metadata_rx.recv().await {
            let _ = broadcast_tx.send(metadata_json);
        }
    });

    // Task to write messages to the WebSocket
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write.send(Message::Text(Utf8Bytes::from(msg))).await.is_err() {
                break; // Client disconnected
            }
        }
    });

    // Task to read from WebSocket and handle requests
    let read_ctx = ctx.clone();
    tokio::spawn(async move {
        while let Some(msg_result) = read.next().await {
            if let Ok(text) = msg_result {
                if text.into_text().unwrap() == "metadata" {
                    if let Ok(json) = get_metadata_json(&read_ctx.player).await {
                        let _ = tx.send(json);
                    }
                }
            }
        }
    });

    Ok(())
}

pub async fn get_metadata_json(player: &OpusPlayerHandle) -> Result<String, String> {
    let metadata = player.get_metadata().await.map_err(|e| e.to_string())?
        .ok_or_else(|| "No active file metadata".to_string())?;
    let TimeData { start_time_ms, current_time_ms } = player.get_time_data().await.map_err(|e| e.to_string())?;

    let json = format!(
        r#"{{"id":"{}","title":"{}","author":"{}","active_file_duration_ms":{},"active_file_start_time_ms":{},"active_file_current_time_ms":{},"buffer_size_ms":{},"image":{}}}"#,
        metadata.id,
        metadata.title,
        metadata.author,
        metadata.duration_ms,
        start_time_ms,
        current_time_ms,
        BUFFER_SIZE_MS,
        if let Some(url) = metadata.image { format!("\"{}\"", url) } else { "null".to_string() },
    );

    Ok(json)
}
