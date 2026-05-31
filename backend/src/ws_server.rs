use std::{net::SocketAddr, sync::Arc};

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::{
    handshake::server::{ErrorResponse, Request as HandshakeRequest, Response as HandshakeResponse},
    Message, Utf8Bytes,
};

use crate::{AppState, opus_player::{BUFFER_SIZE_MS, OpusPlayerHandle, TimeData}};

pub struct WSServerContext {
    pub app: Arc<AppState>,
}

pub async fn init_ws_server(
    port: u16,
    ctx: WSServerContext,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");

    let ctx_arc = Arc::from(ctx);

    println!("WS server is up on port {}", port);

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(accept_connection(ctx_arc.clone(), stream));
    }

    Ok(())
}

async fn accept_connection(
    ctx: Arc<WSServerContext>,
    stream: TcpStream,
) -> Result<(), String> {
    use std::sync::Mutex;
    let captured_path: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let captured_for_cb = captured_path.clone();

    let ws_stream = tokio_tungstenite::accept_hdr_async(stream, move |req: &HandshakeRequest, resp: HandshakeResponse| {
        *captured_for_cb.lock().unwrap() = Some(req.uri().path().to_string());
        Ok::<_, ErrorResponse>(resp)
    })
    .await
    .map_err(|e| format!("ws handshake: {}", e))?;

    let path = captured_path.lock().unwrap().clone().unwrap_or_default();

    // Expect path of the form /streams/{id}
    let stream_id = match path.strip_prefix("/streams/") {
        Some(rest) if !rest.is_empty() => rest.to_string(),
        _ => {
            // Send a close frame and bail.
            let (mut write, _) = ws_stream.split();
            let _ = write
                .send(Message::Close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
                    code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Policy,
                    reason: "Invalid stream path".into(),
                })))
                .await;
            return Ok(());
        }
    };

    let entry_arc = match ctx.app.registry.read().await.get(&stream_id).cloned() {
        Some(e) => e,
        None => {
            let (mut write, _) = ws_stream.split();
            let _ = write
                .send(Message::Close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
                    code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Policy,
                    reason: "Unknown stream".into(),
                })))
                .await;
            return Ok(());
        }
    };

    let (player, mut metadata_rx, stream_name) = {
        let e = entry_arc.read().await;
        (e.player.clone(), e.metadata_tx.subscribe(), e.config.name.clone())
    };

    let (mut write, mut read) = ws_stream.split();

    // Channel to multiplex outgoing messages.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let broadcast_tx = tx.clone();
    tokio::spawn(async move {
        while let Ok(metadata_json) = metadata_rx.recv().await {
            if broadcast_tx.send(metadata_json).is_err() {
                break;
            }
        }
    });

    // Outgoing writer
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write.send(Message::Text(Utf8Bytes::from(msg))).await.is_err() {
                break;
            }
        }
    });

    // Incoming reader: only "metadata" requests are handled.
    let entry_arc_for_read = entry_arc.clone();
    let player_for_read = player.clone();
    let stream_id_for_read = stream_id.clone();
    tokio::spawn(async move {
        while let Some(msg_result) = read.next().await {
            if let Ok(msg) = msg_result {
                if let Ok(text) = msg.into_text() {
                    if text.as_str() == "metadata" {
                        let current_name = entry_arc_for_read.read().await.config.name.clone();
                        if let Ok(json) = get_metadata_json(&player_for_read, Some(&current_name), Some(&stream_id_for_read)).await {
                            let _ = tx.send(json);
                        }
                    }
                }
            }
        }
    });

    let _ = stream_name; // not used here, but the WS handler may send it if desired

    Ok(())
}

pub async fn get_metadata_json(
    player: &OpusPlayerHandle,
    stream_name: Option<&str>,
    stream_id: Option<&str>,
) -> Result<String, String> {
    let metadata = player.get_metadata().await.map_err(|e| e.to_string())?
        .ok_or_else(|| "No active file metadata".to_string())?;
    let TimeData { start_time_ms, current_time_ms } = player.get_time_data().await.map_err(|e| e.to_string())?;
    let paused = player.is_paused().await.unwrap_or(false);

    let stream_id_field = match stream_id {
        Some(id) => format!(r#","stream_id":"{}""#, escape_json(id)),
        None => String::new(),
    };
    let stream_name_field = match stream_name {
        Some(name) => format!(r#","stream_name":"{}""#, escape_json(name)),
        None => String::new(),
    };
    let image_field = match metadata.image {
        Some(url) => format!(r#""{}""#, escape_json(&url)),
        None => "null".to_string(),
    };

    let json = format!(
        r#"{{"id":"{}","title":"{}","author":"{}","active_file_duration_ms":{},"active_file_start_time_ms":{},"active_file_current_time_ms":{},"buffer_size_ms":{},"image":{},"paused":{}{}{}}}"#,
        metadata.id,
        escape_json(&metadata.title),
        escape_json(&metadata.author),
        metadata.duration_ms,
        start_time_ms,
        current_time_ms,
        BUFFER_SIZE_MS,
        image_field,
        paused,
        stream_id_field,
        stream_name_field,
    );

    Ok(json)
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
