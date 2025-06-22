use std::{convert::Infallible, io::{Cursor, Read, Seek, SeekFrom}, net::SocketAddr, sync::Arc};
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full, StreamBody};
use hyper::{body::{self, Bytes, Frame}, server::conn::http1, service::service_fn, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use ogg::{PacketWriteEndInfo, PacketWriter};
use tokio::{net::TcpListener, sync::mpsc};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};

use crate::opus_player::{OpusPlayer, OpusPlayerEvent, OPUS_COMMENTS, OPUS_HEAD};

pub struct HTTPServerContext {
    pub player: Arc<OpusPlayer>,
}

pub async fn init_http_server(
    port: u16,
    ctx: HTTPServerContext,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let listener = TcpListener::bind(addr).await?;
    
    let ctx_arc = Arc::from(ctx);

    println!("Server is up and running on port {}", port);
    loop {
        let (stream, _) = listener.accept().await?;

        let io = TokioIo::new(stream);

        let cloned_ctx = ctx_arc.clone();

        let service = service_fn(move |req| {
            let cloned_ctx = cloned_ctx.clone();
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
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => {
            println!("Connected {:?} {:?}", req.uri().path(), req.headers());
            let (tx, rx) = mpsc::channel(5);

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

            tokio::spawn(async move {
                
                // Send opus head & comments
                tx.send(OpusPlayerEvent::AudioData { raw_opus_data: OPUS_HEAD.to_vec(), granule_position: 0 })
                    .await
                    .expect("Should send opus head");
                tx.send(OpusPlayerEvent::AudioData { raw_opus_data: OPUS_COMMENTS.to_vec(), granule_position: 0 })
                    .await
                    .expect("Should send opus comments");

                let headstart_events = ctx.player.get_headstart_data().await;
                println!("got headstart bullshit {}", headstart_events.len());

                // It's important to send after registering the listener, otherwise
                // the buffer would fill up and get stuck since there's no consumer
                for event in headstart_events {
                    tx.send(event).await.expect("Should send headstart data");
                }

                // Start listening for player data
                ctx.player.add_listener(tx).await;
            });

            let stream_body = StreamBody::new(stream);

            let response = Response::builder()
                .header("Connection", "keep-alive")
                .header("Content-Type", "audio/ogg")
                .body(BoxBody::new(stream_body))
                .expect("Should build body");

            return Ok(response);
        },
        _ => {
            let mut not_found = Response::new(empty());
            *not_found.status_mut() = StatusCode::NOT_FOUND;

            Ok(not_found)
        }
    }
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
