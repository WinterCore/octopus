mod ogg;
mod socket_manager;
mod ogg_player;

use std::{io, sync::Arc};
use tokio::{net::TcpListener, sync::mpsc, io::{AsyncReadExt, AsyncWriteExt}, task::JoinHandle};
use socket_manager::{SocketManagerHandle, SocketManager};
use ogg_player::OggPlayerHandle;

use crate::ogg_player::OggPlayerEvent;

#[tokio::main]
async fn main() -> io::Result<()> {
    let socket_manager = Arc::new(SocketManagerHandle::new());

    println!("Up running on port 8080");
    let ogg_player = Arc::new(OggPlayerHandle::new());

    let ogg_player_controller = ogg_player.clone();
    let controlled_player_handle = tokio::spawn(async move {
        ogg_player_controller.load_file("/home/winter/Downloads/someday-that-summer.opus".to_owned())
            .await
            .expect("Should load file");
    });

    let player_handle = init_player(
        ogg_player.clone(),
        socket_manager.clone(),
    );

    let streaming_server_handle = init_streaming_server(
        ogg_player.clone(),
        socket_manager.clone(),
    );

    let _ = tokio::join!(
        controlled_player_handle,
        player_handle,
        streaming_server_handle,
    );

    Ok(())
}

async fn init_player(
    ogg_player: Arc<OggPlayerHandle>,
    socket_manager: Arc<SocketManagerHandle>,
) -> Result<(), String> {
    tokio::spawn(async move {
        let (tx, mut rx) = mpsc::channel(5);
        ogg_player.add_listener(tx).await.expect("Should subscribe to player");
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

async fn init_streaming_server(
    ogg_player: Arc<OggPlayerHandle>,
    socket_manager: Arc<SocketManagerHandle>,
) -> Result<(), String> {
    let listener = TcpListener::bind("0.0.0.0:8080")
        .await
        .map_err(|x| x.to_string())?;
    tokio::spawn(async move {
        loop {
            let (mut socket, addr) = match listener.accept().await {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("[ERROR]: Failed to accept socket! {:?}", e.to_string());
                    continue;
                },
            };
            let mut buf = vec![];
            socket.read(&mut buf)
                .await
                .expect("Should read socket data");

            println!("[INFO]: Socket connected {}", addr);
            let http_head = "HTTP/1.1 200 OK\r\nConnection: keep-alive\r\nContent-Type: audio/ogg\r\nTransfer-Encoding: chunked\r\n\r\n";
            socket.write_all(http_head.as_bytes())
                .await
                .expect("Should send http head");
            socket.flush()
                .await
                .expect("Should flush socket");

            let head = ogg_player.get_head().await;

            println!("Received head {:?}", head);
            if let Some(data) = head {
                match SocketManager::send_to_socket(&mut socket, &data).await {
                    Err(_) => eprintln!("[ERROR]: Failed to send header pages to client {}", addr),
                    _ => {},
                }
            }

            match socket_manager.register_socket(socket).await {
                Ok(id) => id,
                Err(_) => {
                    eprintln!("[ERROR(SOCKET_MANAGER)]: Failed to register socket");
                    continue;
                },
            };

        }
    }).await
    .map_err(|x| x.to_string())?;

    Ok(())
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

