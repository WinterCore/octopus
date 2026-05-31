use std::{convert::Infallible, io::{Cursor, Read, Seek, SeekFrom}, net::SocketAddr, path::Path, sync::Arc};
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full, StreamBody};
use hyper::{body::{self, Bytes, Frame}, header::{self, HeaderValue}, server::conn::http1, service::service_fn, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use ogg::{PacketWriteEndInfo, PacketWriter};
use tokio::{fs, net::TcpListener, sync::mpsc};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};

use crate::{
    AppState,
    auth::{cookie_clear_value, cookie_header_value, extract_token},
    opus_player::{OPUS_COMMENTS, OPUS_HEAD, OpusPlayerEvent, OpusPlayerHandle},
};

pub struct HTTPServerContext {
    pub app: Arc<AppState>,
}

pub async fn init_http_server(
    port: u16,
    ctx: HTTPServerContext,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let listener = TcpListener::bind(addr).await?;

    let ctx_arc = Arc::from(ctx);

    println!("Server is up and running on port {}", port);
    loop {
        let (stream, socket) = listener.accept().await?;

        let io = TokioIo::new(stream);

        let cloned_ctx = ctx_arc.clone();

        let service = service_fn(move |req| {
            let cloned_ctx = cloned_ctx.clone();
            println!("{} {} from {}", req.method(), req.uri().path(), socket.ip());
            main_handler(cloned_ctx, req)
        });

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}

const SERIAL: u32 = 61;

struct OggStream<'a> {
    writer: PacketWriter<'a, Cursor<Vec<u8>>>,
    output_buffer: Vec<u8>,
}

impl<'a> OggStream<'a> {
    fn new() -> Self {
        let writer_buffer = vec![0u8; 4096 * 2];
        let cursor = Cursor::new(writer_buffer);
        let writer = PacketWriter::new(cursor);
        let output_buffer = vec![0u8; 4096];

        Self { writer, output_buffer }
    }

    fn encode(&mut self, data: Vec<u8>, absgp: u64) -> &[u8] {
        self.writer.inner_mut().seek(SeekFrom::Start(0)).expect("Should seek to start");

        self.writer.write_packet(data, SERIAL, PacketWriteEndInfo::EndPage, absgp)
            .expect("Should encode packet");

        let head_len = self.writer.inner().position() as usize;
        self.writer.inner_mut().seek(SeekFrom::Start(0))
            .expect("Should seek to start");
        self.writer.inner_mut().read_exact(&mut self.output_buffer[..head_len])
            .expect("Should read head into buffer");

        &self.output_buffer[..head_len]
    }
}

async fn main_handler(
    ctx: Arc<HTTPServerContext>,
    req: Request<body::Incoming>
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Infallible> {
    let origin = req.headers().get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let mut response = route_request(ctx, req).await;
    apply_cors_headers(&mut response, origin.as_deref());
    Ok(response)
}

fn apply_cors_headers<B>(response: &mut Response<B>, origin: Option<&str>) {
    let headers = response.headers_mut();
    // For credentialed requests the spec forbids using `*`; we must echo the
    // exact Origin back. We do that for every response when an Origin header
    // is present — the response then works for both credentialed and
    // uncredentialed clients.
    match origin.and_then(|o| HeaderValue::from_str(o).ok()) {
        Some(value) => {
            headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
            headers.insert(header::VARY, HeaderValue::from_static("Origin"));
            headers.insert(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, HeaderValue::from_static("true"));
        }
        None => {
            headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
        }
    }
    headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, PATCH, OPTIONS"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("Content-Type, Authorization"));
}

async fn route_request(
    ctx: Arc<HTTPServerContext>,
    req: Request<body::Incoming>
) -> Response<BoxBody<Bytes, hyper::Error>> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    if method == Method::OPTIONS {
        return cors_preflight();
    }

    // Public stream listing
    if method == Method::GET && path == "/streams" {
        return list_streams_response(&ctx, false).await;
    }

    // /streams/{id}/audio  or  /streams/{id}/playlist-image
    if let Some(rest) = path.strip_prefix("/streams/") {
        let mut parts = rest.splitn(2, '/');
        let id = parts.next().unwrap_or("").to_string();
        let suffix = parts.next().unwrap_or("");

        if !id.is_empty() && (suffix == "audio" || suffix.is_empty()) {
            if method == Method::GET {
                return stream_audio_response(&ctx, &id).await;
            }
        }

        if !id.is_empty() && suffix == "playlist-image" {
            if method == Method::GET || method == Method::HEAD {
                return playlist_image_response(&ctx, &id).await;
            }
        }
    }

    // Admin
    if method == Method::POST && path == "/admin/login" {
        return admin_login(&ctx, req).await;
    }
    if method == Method::POST && path == "/admin/logout" {
        return admin_logout(&ctx, req).await;
    }
    if method == Method::GET && path == "/admin/streams" {
        if !require_session(&ctx, &req) {
            return unauthorized();
        }
        return list_streams_response(&ctx, true).await;
    }

    if let Some(rest) = path.strip_prefix("/admin/streams/") {
        if !require_session(&ctx, &req) {
            return unauthorized();
        }

        let mut parts = rest.splitn(2, '/');
        let id = parts.next().unwrap_or("").to_string();
        let action = parts.next().unwrap_or("");

        if id.is_empty() {
            return not_found();
        }

        match (method.clone(), action) {
            (Method::POST, "skip") => return admin_skip(&ctx, &id).await,
            (Method::POST, "pause") => return admin_pause(&ctx, &id).await,
            (Method::POST, "resume") => return admin_resume(&ctx, &id).await,
            (Method::PATCH, "") => return admin_rename(&ctx, &id, req).await,
            _ => {}
        }
    }

    not_found()
}

async fn list_streams_response(ctx: &Arc<HTTPServerContext>, include_playlist: bool) -> Response<BoxBody<Bytes, hyper::Error>> {
    let registry = ctx.app.registry.read().await;

    let mut items = Vec::with_capacity(registry.len());
    for (_, entry_arc) in registry.iter() {
        let entry = entry_arc.read().await;
        let paused = entry.player.is_paused().await.unwrap_or(false);
        let metadata = entry.player.get_metadata().await.ok().flatten();

        let title_field = match metadata.as_ref() {
            Some(m) => format!(r#","title":"{}""#, json_escape(&m.title)),
            None => String::new(),
        };
        let author_field = match metadata.as_ref() {
            Some(m) => format!(r#","author":"{}""#, json_escape(&m.author)),
            None => String::new(),
        };
        let playlist_field = if include_playlist {
            format!(r#","playlist":"{}""#, json_escape(&entry.config.playlist))
        } else {
            String::new()
        };

        items.push(format!(
            r#"{{"id":"{}","name":"{}","paused":{}{}{}{}}}"#,
            json_escape(&entry.config.id),
            json_escape(&entry.config.name),
            paused,
            title_field,
            author_field,
            playlist_field,
        ));
    }
    drop(registry);

    let body = format!("[{}]", items.join(","));
    json_ok(body)
}

async fn stream_audio_response(ctx: &Arc<HTTPServerContext>, stream_id: &str) -> Response<BoxBody<Bytes, hyper::Error>> {
    let player = match get_player(ctx, stream_id).await {
        Some(p) => p,
        None => return not_found(),
    };

    let (tx, rx) = mpsc::channel(500);
    let mut ogg_stream = OggStream::new();

    let stream = ReceiverStream::new(rx)
        .map(move |player_event| {
            match player_event {
                OpusPlayerEvent::AudioData { raw_opus_data, granule_position } => {
                    let ogg_data = ogg_stream.encode(raw_opus_data, granule_position);
                    Ok(Frame::data(Bytes::from(ogg_data.to_vec())))
                },
            }
        });

    let player_for_task = player.clone();
    tokio::spawn(async move {
        tx.send(OpusPlayerEvent::AudioData { raw_opus_data: OPUS_HEAD.to_vec(), granule_position: 0 })
            .await
            .expect("Should send opus head");
        tx.send(OpusPlayerEvent::AudioData { raw_opus_data: OPUS_COMMENTS.to_vec(), granule_position: 0 })
            .await
            .expect("Should send opus comments");

        let headstart_events = player_for_task.get_headstart_data().await.expect("Should get headstart data");

        for event in headstart_events {
            tx.send(event).await.expect("Should send headstart data");
        }

        player_for_task.register_listener(tx).await.expect("Should register listener");
    });

    let stream_body = StreamBody::new(stream);

    Response::builder()
        .header("Connection", "keep-alive")
        .header("Content-Type", "audio/ogg")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS")
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "Content-Type, Authorization")
        .header(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, "true")
        .body(BoxBody::new(stream_body))
        .expect("Should build body")
}

async fn playlist_image_response(ctx: &Arc<HTTPServerContext>, stream_id: &str) -> Response<BoxBody<Bytes, hyper::Error>> {
    let player = match get_player(ctx, stream_id).await {
        Some(p) => p,
        None => return not_found(),
    };

    let playlist_path = match player.get_playlist_path().await {
        Ok(Some(p)) => p,
        _ => return cors_response(StatusCode::NOT_FOUND, empty()),
    };

    let image_path = Path::new(&playlist_path).join("playlist.jpg");

    match fs::read(&image_path).await {
        Ok(image_data) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/jpeg")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(full(image_data))
            .expect("Should build response"),
        Err(_) => cors_response(StatusCode::NOT_FOUND, empty()),
    }
}

async fn admin_login(ctx: &Arc<HTTPServerContext>, req: Request<body::Incoming>) -> Response<BoxBody<Bytes, hyper::Error>> {
    let body = match req.collect().await {
        Ok(c) => c.to_bytes(),
        Err(_) => return cors_response(StatusCode::BAD_REQUEST, empty()),
    };

    let password = extract_json_string_field(&body, "password").unwrap_or_default();

    if !ctx.app.auth.check_password(&password) {
        return cors_response(StatusCode::UNAUTHORIZED, full(r#"{"error":"invalid password"}"#));
    }

    let token = ctx.app.auth.issue_session();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, "true")
        .header(header::SET_COOKIE, cookie_header_value(&token))
        .body(full(r#"{"ok":true}"#))
        .expect("Should build response")
}

async fn admin_logout(ctx: &Arc<HTTPServerContext>, req: Request<body::Incoming>) -> Response<BoxBody<Bytes, hyper::Error>> {
    if let Some(token) = extract_token(&req) {
        ctx.app.auth.revoke(&token);
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, "true")
        .header(header::SET_COOKIE, cookie_clear_value())
        .body(full(r#"{"ok":true}"#))
        .expect("Should build response")
}

async fn admin_skip(ctx: &Arc<HTTPServerContext>, stream_id: &str) -> Response<BoxBody<Bytes, hyper::Error>> {
    let player = match get_player(ctx, stream_id).await {
        Some(p) => p,
        None => return not_found(),
    };
    if let Err(e) = player.skip().await {
        return cors_response(StatusCode::INTERNAL_SERVER_ERROR, full(format!(r#"{{"error":"{}"}}"#, json_escape(&e))));
    }
    cors_response(StatusCode::OK, full(r#"{"ok":true}"#))
}

async fn admin_pause(ctx: &Arc<HTTPServerContext>, stream_id: &str) -> Response<BoxBody<Bytes, hyper::Error>> {
    let entry_arc = match ctx.app.registry.read().await.get(stream_id).cloned() {
        Some(e) => e,
        None => return not_found(),
    };
    let (player, metadata_tx, stream_name) = {
        let e = entry_arc.read().await;
        (e.player.clone(), e.metadata_tx.clone(), e.config.name.clone())
    };
    if let Err(e) = player.pause().await {
        return cors_response(StatusCode::INTERNAL_SERVER_ERROR, full(format!(r#"{{"error":"{}"}}"#, json_escape(&e))));
    }
    // Broadcast updated metadata so listeners see the paused flag.
    if let Ok(json) = crate::ws_server::get_metadata_json(&player, Some(&stream_name), Some(stream_id)).await {
        let _ = metadata_tx.send(json);
    }
    cors_response(StatusCode::OK, full(r#"{"ok":true}"#))
}

async fn admin_resume(ctx: &Arc<HTTPServerContext>, stream_id: &str) -> Response<BoxBody<Bytes, hyper::Error>> {
    let entry_arc = match ctx.app.registry.read().await.get(stream_id).cloned() {
        Some(e) => e,
        None => return not_found(),
    };
    let (player, metadata_tx, stream_name) = {
        let e = entry_arc.read().await;
        (e.player.clone(), e.metadata_tx.clone(), e.config.name.clone())
    };
    if let Err(e) = player.resume().await {
        return cors_response(StatusCode::INTERNAL_SERVER_ERROR, full(format!(r#"{{"error":"{}"}}"#, json_escape(&e))));
    }
    if let Ok(json) = crate::ws_server::get_metadata_json(&player, Some(&stream_name), Some(stream_id)).await {
        let _ = metadata_tx.send(json);
    }
    cors_response(StatusCode::OK, full(r#"{"ok":true}"#))
}

async fn admin_rename(ctx: &Arc<HTTPServerContext>, stream_id: &str, req: Request<body::Incoming>) -> Response<BoxBody<Bytes, hyper::Error>> {
    let body = match req.collect().await {
        Ok(c) => c.to_bytes(),
        Err(_) => return cors_response(StatusCode::BAD_REQUEST, empty()),
    };

    let new_name = match extract_json_string_field(&body, "name") {
        Some(n) if !n.trim().is_empty() => n,
        _ => return cors_response(StatusCode::BAD_REQUEST, full(r#"{"error":"name required"}"#)),
    };

    let entry_arc = match ctx.app.registry.read().await.get(stream_id).cloned() {
        Some(e) => e,
        None => return not_found(),
    };

    // Update in-memory name and broadcast.
    {
        let mut entry = entry_arc.write().await;
        entry.config.name = new_name.clone();
    }
    let (player, metadata_tx, stream_name) = {
        let e = entry_arc.read().await;
        (e.player.clone(), e.metadata_tx.clone(), e.config.name.clone())
    };
    if let Ok(json) = crate::ws_server::get_metadata_json(&player, Some(&stream_name), Some(stream_id)).await {
        let _ = metadata_tx.send(json);
    }

    // Persist to disk: snapshot the registry into a fresh StreamsConfig and save.
    if let Err(e) = persist_config(&ctx.app).await {
        eprintln!("Failed to persist config after rename: {}", e);
    }

    cors_response(StatusCode::OK, full(r#"{"ok":true}"#))
}

async fn persist_config(app: &AppState) -> Result<(), String> {
    let registry = app.registry.read().await;
    let mut streams = Vec::with_capacity(registry.len());
    for (_, entry_arc) in registry.iter() {
        let entry = entry_arc.read().await;
        streams.push(entry.config.clone());
    }
    drop(registry);
    let cfg = crate::config::StreamsConfig {
        default_stream: app.default_stream.clone(),
        streams,
    };
    cfg.save(&app.config_path)
}

async fn get_player(ctx: &Arc<HTTPServerContext>, stream_id: &str) -> Option<OpusPlayerHandle> {
    let entry = ctx.app.registry.read().await.get(stream_id).cloned()?;
    let player = entry.read().await.player.clone();
    Some(player)
}

fn require_session(ctx: &Arc<HTTPServerContext>, req: &Request<body::Incoming>) -> bool {
    match extract_token(req) {
        Some(token) => ctx.app.auth.validate(&token),
        None => false,
    }
}

fn unauthorized() -> Response<BoxBody<Bytes, hyper::Error>> {
    cors_response(StatusCode::UNAUTHORIZED, full(r#"{"error":"unauthorized"}"#))
}

fn not_found() -> Response<BoxBody<Bytes, hyper::Error>> {
    cors_response(StatusCode::NOT_FOUND, empty())
}

fn cors_preflight() -> Response<BoxBody<Bytes, hyper::Error>> {
    let mut response = Response::new(empty());
    *response.status_mut() = StatusCode::NO_CONTENT;
    let h = response.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    h.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, PATCH, OPTIONS"));
    h.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("Content-Type, Authorization"));
    h.insert(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, HeaderValue::from_static("true"));
    response
}

fn cors_response(status: StatusCode, body: BoxBody<Bytes, hyper::Error>) -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, "true")
        .header(header::CONTENT_TYPE, "application/json")
        .body(body)
        .expect("Should build response")
}

fn json_ok(body: String) -> Response<BoxBody<Bytes, hyper::Error>> {
    cors_response(StatusCode::OK, full(body))
}

fn json_escape(s: &str) -> String {
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

/// Minimal JSON string-field extractor. Looks for `"field":"value"` in the body.
/// Handles backslash escapes for `\"` and `\\`. Returns None if not found.
fn extract_json_string_field(body: &[u8], field: &str) -> Option<String> {
    let s = std::str::from_utf8(body).ok()?;
    let needle = format!("\"{}\"", field);
    let start = s.find(&needle)? + needle.len();
    let rest = &s[start..];
    let colon = rest.find(':')? + 1;
    let rest = &rest[colon..];
    let trimmed = rest.trim_start();
    let bytes = trimmed.as_bytes();
    if bytes.first().copied() != Some(b'"') {
        return None;
    }
    let mut out = String::new();
    let mut iter = trimmed[1..].chars();
    while let Some(c) = iter.next() {
        match c {
            '\\' => {
                let next = iter.next()?;
                match next {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    other => out.push(other),
                }
            },
            '"' => return Some(out),
            other => out.push(other),
        }
    }
    None
}

fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}
