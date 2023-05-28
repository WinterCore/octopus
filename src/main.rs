mod ogg;

use std::time::Duration;
use std::{io, fs};
use ogg::{OggPage, OggParser};
use tokio::time::sleep;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{BufReader, BufWriter, AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Up running on port 8080");

    loop {
        let (mut socket, _) = listener.accept().await?;

        tokio::spawn(async move {
            match process_socket(&mut socket).await {
                Err(err) => {
                    eprintln!("[ERROR]: Something happened while sending response {:?}", err);
                },
                Ok(()) => {
                    println!("[INFO]: Response was sent successfully!");
                },
            };
        });
    }
}

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
    reader.read(&mut buffer).await?;

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



