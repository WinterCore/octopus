mod ogg_parser;

use std::{io, fs, str};
use ogg_parser::{OggPage, OggParser};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{BufReader, BufWriter, AsyncReadExt, AsyncWriteExt};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> io::Result<()> {

    /*
    println!("File has {} pages", pages.len());
    pages.splice(4..10, []);
    let serialized: Vec<u8> = pages.iter().flat_map(|x| x.serialize()).collect();

    fs::write("/home/winter/Downloads/mysunset-output.opus", serialized)
        .expect("Failed to write output file!");
    */

    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (mut socket, _) = listener.accept().await?;

        match process_socket(&mut socket).await {
            Err(err) => {
                eprintln!("[ERROR]: Something happened while sending response {:?}", err);
            },
            Ok(()) => {
                println!("[INFO]: Response was sent successfully!");
            },
        };
    }
}

async fn process_socket(stream: &mut TcpStream) -> io::Result<()> {
    let ip = stream.peer_addr().expect("Stream has peer_addr");
    println!("[INFO]: Client connected {}", ip);

    let (reader, writer) = stream.split();
    
    let data = fs::read("/home/winter/Downloads/someday-that-summer.opus")
        .expect("Could not read audio file");
    let mut pages: Vec<OggPage> = OggParser::new(&data).into_iter().collect();

    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);
    let mut buffer: Vec<u8> = vec![0; 500];
    reader.read(&mut buffer).await?;
    let data = str::from_utf8(&buffer).unwrap();
    let serialized: Vec<u8> = pages.iter().flat_map(|x| x.serialize()).collect();
    let response = "HTTP/1.1 200 OK\r\nContent-Type: audio/ogg\r\nTransfer-Encoding: chunked\r\n\r\n";
    // let response = format!("HTTP/1.1 200 OK\r\nContent-Type: audio/ogg\r\nContent-Length: {}\r\n\r\n", serialized.len());

    writer.write(response.as_bytes()).await?;

    for page in pages[..2].iter() {
        let binary = page.serialize();
        // Write length
        writer.write(format!("{:X}", binary.len()).as_bytes()).await?;
        writer.write("\r\n".as_bytes()).await?;
        writer.write(&page.serialize()).await?;
        writer.write("\r\n".as_bytes()).await?;
        // sleep(Duration::from_millis(2000)).await;
    }
    for page in pages[200..].iter() {
        let binary = page.serialize();
        // Write length
        writer.write(format!("{:X}", binary.len()).as_bytes()).await?;
        writer.write("\r\n".as_bytes()).await?;
        writer.write(&page.serialize()).await?;
        writer.write("\r\n".as_bytes()).await?;
        // sleep(Duration::from_millis(2000)).await;
    }
    writer.write("0".as_bytes()).await?;
    writer.write("\r\n".as_bytes()).await?;
    writer.write("\r\n".as_bytes()).await?;
    writer.flush().await?;

    Ok(())
}



