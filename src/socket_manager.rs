use std::collections::HashMap;
use std::str;

use tokio::{net::TcpStream, io::{AsyncWriteExt, self}, sync::{oneshot, mpsc}};

#[derive(Debug)]
pub struct SocketManager {
    receiver: mpsc::Receiver<SocketManagerMessage>,
    sockets: HashMap<usize, TcpStream>,
    id_counter: usize,
}

#[derive(Debug)]
enum SocketManagerMessage {
    SendAll(Vec<u8>),
    Send(usize, Vec<u8>),
    RegisterSocket {
        socket: TcpStream,
        sender_cb: oneshot::Sender<usize>,
    },
}

impl SocketManager {
    fn new(receiver: mpsc::Receiver<SocketManagerMessage>) -> Self {
        Self { receiver, id_counter: 0, sockets: HashMap::new() }
    }

    async fn send_all(&mut self, buf: &[u8]) -> Result<(), String> {
        let mut sockets_to_clean: Vec<usize> = vec![];

        for (id, socket) in self.sockets.iter_mut() {
            match Self::send_to_socket(socket, buf).await {
                Err(e) => {
                    // Failed to write to socket
                    sockets_to_clean.push(*id);
                    eprintln!("[ERROR]: Something happened while writing to socket! {:?}", e);
                },
                _ => {},
            };
        }

        sockets_to_clean
            .into_iter()
            .for_each(|id| {
                self.sockets.remove(&id);
            });

        Ok(())
    }

    pub async fn send_to_socket(socket: &mut TcpStream, buf: &[u8]) -> Result<(), String> {
        let result = async {
            socket.write(format!("{:X}", buf.len()).as_bytes()).await?;
            socket.write("\r\n".as_bytes()).await?;
            socket.write_all(buf).await?;
            socket.write("\r\n".as_bytes()).await?;
            socket.flush().await?;

            Ok::<(), io::Error>(())
        };

        return result
            .await
            .map_err(|x| x.to_string())
    }

    async fn send(&mut self, id: usize, buf: &[u8]) -> Result<(), String> {
        if let Some(socket) = self.sockets.get_mut(&id) {
            match Self::send_to_socket(socket, buf).await {
                Err(e) => {
                    // Failed to write to socket
                    self.sockets.remove(&id);
                    eprintln!("[ERROR]: Something happened while writing to socket! {:?}", e);
                },
                _ => {},
            };
        }

        Ok(())
    }

    fn register_socket(&mut self, socket: TcpStream) -> usize {
        let id = self.id_counter;
        self.sockets.insert(id, socket);
        self.id_counter += 1;

        id
    }

    async fn handle_message(&mut self, msg: SocketManagerMessage) -> Result<(), String> {
        match msg {
            SocketManagerMessage::SendAll(data) => {
                self.send_all(&data).await?;
            },
            SocketManagerMessage::Send(id, data) => {
                self.send(id, &data).await?;
            },
            SocketManagerMessage::RegisterSocket { socket, sender_cb } => {
                let id = self.register_socket(socket);
                sender_cb.send(id).expect("Should send response");
            },
        };

        Ok(())
    }
}

async fn run_socket_manager(mut sm: SocketManager) {
    while let Some(msg) = sm.receiver.recv().await {
        if let Err(e) = sm.handle_message(msg).await {
            eprintln!("[ERROR(socket_manager)]: Failed to process message {}", e);
        }
    }
}

#[derive(Debug)]
pub struct SocketManagerHandle {
    sender: mpsc::Sender<SocketManagerMessage>,
}

// TODO: Come up with better errors
#[derive(Debug)]
pub enum SocketManagerError {
    SendError,
}

impl SocketManagerHandle {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(50);
        let actor = SocketManager::new(rx);
        tokio::spawn(run_socket_manager(actor));

        Self { sender: tx }
    }

    pub async fn register_socket(&self, socket: TcpStream) -> Result<usize, SocketManagerError> {
        let (tx, rx) = oneshot::channel();

        self.sender.send(SocketManagerMessage::RegisterSocket { socket, sender_cb: tx })
            .await
            .map_err(|_| SocketManagerError::SendError)?;

        Ok(rx.await.expect("Task has been killed"))
    }

    pub async fn send(&self, id: usize, data: Vec<u8>) -> Result<(), SocketManagerError> {
        self.sender.send(SocketManagerMessage::Send(id, data))
            .await
            .map_err(|_| SocketManagerError::SendError)?;

        Ok(())
    }

    pub async fn send_all(&self, data: Vec<u8>) -> Result<(), SocketManagerError> {
        self.sender.send(SocketManagerMessage::SendAll(data))
            .await
            .map_err(|_| SocketManagerError::SendError)?;

        Ok(())
    }
}
