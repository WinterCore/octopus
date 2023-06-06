mod ogg;
mod socket_manager;
mod ogg_player;

use std::{io, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::mpsc, io::{AsyncReadExt, AsyncWriteExt}, time::sleep};
use socket_manager::{SocketManagerHandle, SocketManager};
use ogg_player::OggPlayer;

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    let _socket_manager = Arc::new(SocketManagerHandle::new());

    println!("Up running on port 8080");
    let _ogg_player = Arc::new(OggPlayer::new());

    let player_ogg_player = _ogg_player.clone();
    let server_ogg_player = _ogg_player.clone();

    let controlled_player_handle = tokio::spawn(async move {
        player_ogg_player.load_file("/home/winter/Downloads/someday-that-summer.opus")
            .await
            .expect("Should load file");

        player_ogg_player.play().await.expect("Should play file");
    });

    let player_socket_manager = _socket_manager.clone();
    let player_handle = tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            println!("Received player data");
            match player_socket_manager.send_all(data).await {
                Err(_) => eprintln!("[ERROR]: Failed to send ogg page to all sockets!"),
                _ => {},
            };
        }
    });

    let server_socket_manager = _socket_manager.clone();
    let server_handle = tokio::spawn(async move {
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

            let head = server_ogg_player.get_head().await;

            if let Some(data) = head {
                match SocketManager::send_to_socket(&mut socket, &data).await {
                    Err(_) => eprintln!("[ERROR]: Failed to send header pages to client {}", addr),
                    _ => {},
                }
            }

            match server_socket_manager.register_socket(socket).await {
                Ok(id) => id,
                Err(_) => {
                    eprintln!("[ERROR(SOCKET_MANAGER)]: Failed to register socket");
                    continue;
                },
            };

        }
    });

    let _ = tokio::join!(
        controlled_player_handle,
        player_handle,
        server_handle,
    );

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
