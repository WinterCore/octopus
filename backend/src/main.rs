mod socket_manager;
mod ogg_player;
mod oeggs;

use std::{env, io::{self, Cursor, Seek, SeekFrom, Write}, net::SocketAddr, sync::Arc};
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use ogg::{PacketWriteEndInfo, PacketWriter};
use tokio::{io::{copy, AsyncReadExt, AsyncWriteExt}, net::TcpListener, sync::mpsc, time::{sleep, Duration}};
use socket_manager::{SocketManagerHandle, SocketManager};
use ogg_player::OggPlayer;

use crate::ogg_player::OggPlayerEvent;

#[tokio::main]
async fn main() -> io::Result<()> {
    // let socket_manager = Arc::new(SocketManagerHandle::new());
    let port: String = env::var("PORT").expect("Should specify a PORT env variable");

    println!("Up running on port {port}");
    let ogg_player = Arc::new(OggPlayer::new());

    let streaming_server_handle = init_streaming_server(
        &port,
        ogg_player.clone(),
    );

    let ogg_player_controller = ogg_player.clone();
    let ogg_player_controller2 = ogg_player.clone();

    let controlled_player_handle = tokio::spawn(async move {
        ogg_player_controller.load_file("/home/winter/birb.opus")
            .await
            .expect("Should load file");
    });
    let controlled_player2_handle = tokio::spawn(async move {
        sleep(Duration::from_secs(40)).await;
        ogg_player_controller2.load_file("/home/winter/someday-that-summer.opus")
            .await
            .expect("Should load file");
    });

    let _ = tokio::join!(
        controlled_player_handle,
        controlled_player2_handle,
        streaming_server_handle,
    );

    Ok(())
}

/*
async fn init_player(
    ogg_player: Arc<OggPlayer>,
) -> Result<(), String> {
    tokio::spawn(async move {
        let (tx, mut rx) = mpsc::channel(5);
        ogg_player.add_listener(tx).await;
        while let Some(data) = rx.recv().await {
            println!("Received player data");

            let OggPlayerEvent::AudioData { data, timestamp: _ } = data;
            match socket_manager.send_all(data).await {
                Err(_) => eprintln!("[ERROR]: Failed to send ogg page to all sockets!"),
                _ => {},
            };
        }
    }).await
    .map_err(|x| x.to_string())?;

    Ok(())
}
*/


const OPUS_HEAD: &[u8] = &[
    // "OpusHead"
    0x4F, 0x70, 0x75, 0x73, 0x48, 0x65, 0x61, 0x64,
    // Version
    0x01,
    // Channels (stereo)
    0x02,
    // Pre-skip (0)

    0x00, 0x00,
    // Original sample rate (48000 Hz)
    0x80, 0xBB, 0x00, 0x00,
    // Gain (0 dB)

    0x00, 0x00,
    // Channel mapping (0 = mono/stereo)

    0x00,
];

const OPUS_COMMENTS: &[u8] = &[
    // "OpusTags" magic signature
    0x4F, 0x70, 0x75, 0x73, 0x54, 0x61, 0x67, 0x73, // "OpusTags"

    // Vendor string length (7 bytes: "Octopus")
    0x07, 0x00, 0x00, 0x00,

    // Vendor string bytes: "Octopus"
    0x4F, 0x63, 0x74, 0x6F, 0x70, 0x75, 0x73,

    // User comment list length = 0
    0x00, 0x00, 0x00, 0x00,
];

async fn init_streaming_server(
    port: &str,
    ogg_player: Arc<OggPlayer>,
) -> Result<(), String> {

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    
    let listener = TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .map_err(|x| x.to_string())?;


     loop {
        let (stream, _) = listener.accept().await?;

        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(io, service_fn(hello))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }

    loop {
        let (mut socket, addr) = match listener.accept().await {
            Ok(data) => data,
            Err(e) => {
                eprintln!("[ERROR]: Failed to accept socket! {:?}", e.to_string());
                continue;
            },
        };
        println!("Socket connected! {:?}", addr);

        let socket_player = ogg_player.clone();

        tokio::spawn(async move {
            let (tx, mut rx) = mpsc::channel(5);

            // Send ogg header
            let buffer = vec![0u8; 4096 * 2];
            let cursor = Cursor::new(buffer);

            let mut temp_buffer = vec![0u8; 500];
            let mut buf = vec![0u8; 10000];
            let len_read = socket.read(&mut buf)
                .await
                .expect("Should read socket data");

            println!("RESPONSE {:?}", String::from_utf8(buf[..len_read].to_vec()));

            let http_head = "HTTP/1.1 200 OK\r\nConnection: keep-alive\r\nContent-Type: audio/ogg\r\nTransfer-Encoding: chunked\r\n\r\n";
            socket.write_all(http_head.as_bytes())
                .await
                .expect("Should send http head");
            socket.flush()
                .await
                .expect("Should flush socket");

            let serial = 15u32;

            let mut writer = PacketWriter::new(cursor);

            writer.write_packet(OPUS_HEAD, serial, PacketWriteEndInfo::EndPage, 0).expect("Should write opus head");
            writer.write_packet(OPUS_COMMENTS, serial, PacketWriteEndInfo::EndPage, 0).expect("Should write opus comments");
            
            let opus_head_len = writer.inner().position() as usize;
            writer.inner_mut().seek(SeekFrom::Start(0)).expect("Should seek to start");

            writer.inner_mut().read_exact(&mut temp_buffer[..opus_head_len as usize]).await.expect("Should read exact bytes");

            socket.write(format!("{:X}", opus_head_len).as_bytes()).await.expect("ERR");
            socket.write("\r\n".as_bytes()).await.expect("ERR");
            socket.write_all(&mut temp_buffer[..opus_head_len]).await.expect("Should write to socket");
            socket.write("\r\n".as_bytes()).await.expect("ERR");
            socket.flush().await.expect("ERR");

            socket_player.add_listener(tx).await;

            while let Some(data) = rx.recv().await {
                let OggPlayerEvent::AudioData { raw_opus_data, granule_position } = data;

                writer.inner_mut().seek(SeekFrom::Start(0)).expect("Should seek vector to start");
                writer.write_packet(raw_opus_data, serial, PacketWriteEndInfo::EndPage, granule_position).expect("Should write opus packet");
                println!("WRITING {:?}", granule_position);


                let page_len = writer.inner().position() as usize;
                writer.inner_mut().seek(SeekFrom::Start(0)).expect("Should seek vector to start");

                writer.inner_mut().read_exact(&mut temp_buffer[..page_len as usize]).await.expect("Should read exact bytes");

                socket.write(format!("{:X}", page_len).as_bytes()).await.expect("ERR");
                socket.write("\r\n".as_bytes()).await.expect("ERR");
                socket.write_all(&mut temp_buffer[..page_len]).await.expect("Should write to socket");
                socket.write("\r\n".as_bytes()).await.expect("ERR");
                socket.flush().await.expect("ERR");
            }
        });
    }
}

/*
async fn process_socket(stream: &mut TcpStream) -> io::Result<()> {
    let ip = stream.peer_addr().expect("Stream has peer_addr");
    println!("[INFO]: Client connected {}", ip);

    let (reader, writer) = stream.split();
    
    let data1 = fs::read("/home/winter/Downloads/someday-that-summer.opus")
        .expect("Could not read audio file");

    // let data2 = fs::read("/home/winter/Downloads/when-you-were-young.opus")
    //   .expect("Could not read audio file");
    let pages1: Vec<OggPage> = OggParser::new(&data1).into_iter().collect();
    // let pages2: Vec<OggPage> = OggParser::new(&data2).into_iter().collect();

    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);
    let mut buffer: Vec<u8> = vec![0; 500];
    // reader.read(&mut buffer).await?;

    /*
    let data = str::from_utf8(&buffer).unwrap();
    let serialized: Vec<u8> = pages1.iter().flat_map(|x| x.serialize()).collect();
    // let response = format!("HTTP/1.1 200 OK\r\nContent-Type: audio/ogg\r\nContent-Length: {}\r\n\r\n", serialized.len());
    */

    let response = "HTTP/1.1 200 OK\r\nConnection: keep-alive\r\nContent-Type: audio/ogg\r\nTransfer-Encoding: chunked\r\n\r\n";
    writer.write(response.as_bytes()).await?;

    let mut prev_timestamp = 0f64;
    for page in pages1[..100].iter() {
        let binary = page.serialize();
        // Write length
        writer.write(format!("{:X}", binary.len()).as_bytes()).await?;
        writer.write("\r\n".as_bytes()).await?;
        writer.write_all(&page.serialize()).await?;
        writer.write("\r\n".as_bytes()).await?;
        writer.flush().await?;

        let page_timestamp = page.granule_position as f64 / 48_000f64;
        println!("Data: {:?} {:?} | sleep: {:?}", prev_timestamp, page_timestamp, page_timestamp - prev_timestamp);
        sleep(Duration::from_millis(((page_timestamp - prev_timestamp) * 1000f64).round() as u64)).await;
        prev_timestamp = page_timestamp;
    }

    /*
    println!("{:?}", pages2.len());
    for page in pages2[100..120].iter() {
        let binary = page.serialize();
        let mut cloned_page = (*page).clone();
        cloned_page.serial_number = pages1[0].serial_number;
        // Write length
        writer.write(format!("{:X}", binary.len()).as_bytes()).await?;
        writer.write("\r\n".as_bytes()).await?;
        writer.write_all(&cloned_page.serialize()).await?;
        writer.write("\r\n".as_bytes()).await?;
    }
    */
    
    writer.write("0".as_bytes()).await?;
    writer.write("\r\n".as_bytes()).await?;
    writer.write("\r\n".as_bytes()).await?;
    writer.flush().await?;

    Ok(())
}
*/

