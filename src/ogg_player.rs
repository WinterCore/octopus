use std::time::Duration;

use tokio::{fs, sync::{mpsc, oneshot}, time::sleep};

use crate::ogg::{OggPage, OggParser};

enum OggPlayerMessage {
    Play { path: String },
    AddListener(mpsc::Sender<OggPlayerEvent>),
    GetHead(oneshot::Sender<Option<Vec<u8>>>),
}

pub enum OggPlayerEvent {
    AudioData {
        data: Vec<u8>,
        timestamp: u64,
    },
}


struct OggPlayer {
    ogg_data: Option<Vec<OggPage>>,
    curr_page_idx: usize,
    prev_page_timestamp: u64,
    listeners: Vec<mpsc::Sender<OggPlayerEvent>>,
}

impl OggPlayer {
    pub fn new() -> Self {
        Self {
            ogg_data: None,
            curr_page_idx: 0,
            prev_page_timestamp: 0,
            listeners: vec![],
        }
    }

    pub fn get_head(&self) -> Option<Vec<u8>> {
        let bin: Vec<_> = self.ogg_data.as_ref()?
            .iter()
            .take_while(|p| {
                let first_segment = match p.segments.get(0) {
                    None => return false,
                    Some(segment) => segment,
                };

                // TODO: this is ugly but it works
                if first_segment.data.len() >= 8 {
                    let magic_sig = &first_segment.data[..8];

                    magic_sig == "OpusHead".as_bytes()
                    || magic_sig == "OpusTags".as_bytes()
                } else {
                    false
                }
            })
            .map(|x| x.serialize())
            .flatten()
            .collect();

        Some(bin)
    }

    pub async fn load_file(&mut self, path: &str) -> Result<(), String> {
        let data = fs::read(path)
            .await
            .map_err(|x| x.to_string())?;

        self.ogg_data = Some(OggParser::new(&data).into_iter().collect());
        self.curr_page_idx = 0;
        self.play().await?;

        Ok(())
    }

    pub async fn play(&mut self) -> Result<(), String> {
        let buffer_millis = 2000f64;

        loop {
            let i = self.curr_page_idx;
            self.curr_page_idx += 1;
            let data = match self.ogg_data.as_ref() {
                None => return Err("There's no loaded file to be played!".to_owned()),
                Some(data) => data,
            };

            let page = match data.get(i).cloned() {
                Some(page) => page,
                None => break,
            };
            drop(data);

            let page_timestamp = page.granule_position as f64 / 48_000f64 * 1000f64;
            let prev_page_timestamp = self.prev_page_timestamp as f64;

            for listener in self.listeners.iter() {
                listener
                    .send(OggPlayerEvent::AudioData {
                        data: page.serialize(),
                        timestamp: self.prev_page_timestamp,
                    })
                    .await
                    .map_err(|x| x.to_string())?;
            }

            let sleep_duration = page_timestamp - prev_page_timestamp;

            println!("Sleep duration: {} {}", prev_page_timestamp, sleep_duration);
            if sleep_duration > buffer_millis {
                sleep(Duration::from_millis((sleep_duration - buffer_millis) as u64)).await;
                let prev_page_timestamp = (prev_page_timestamp + (sleep_duration - buffer_millis)) as u64;
                println!("Storing {}", prev_page_timestamp);
                self.prev_page_timestamp = prev_page_timestamp;
            }
        }

        Ok(())
    }

    async fn handle_message(&mut self, msg: OggPlayerMessage) -> Result<(), String> {
        match msg {
            OggPlayerMessage::Play { path } => {
                self.load_file(&path).await?;
            },
            OggPlayerMessage::AddListener(listener) => {
                self.listeners.push(listener);
            },
            OggPlayerMessage::GetHead(cb) => {
                cb.send(self.get_head()).expect("Should execute cb");
            },
        };

        Ok(())
    }
}

async fn run_ogg_player(mut op: OggPlayer, mut receiver: mpsc::Receiver<OggPlayerMessage>) {
    while let Some(msg) = receiver.recv().await {
        if let Err(e) = op.handle_message(msg).await {
            eprintln!("[ERROR(socket_manager)]: Failed to process message {}", e);
        }
    }
}

pub struct OggPlayerHandle {
    sender: mpsc::Sender<OggPlayerMessage>,
}

impl OggPlayerHandle {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(50);
        let actor = OggPlayer::new();
        tokio::spawn(run_ogg_player(actor, rx));

        Self { sender: tx }
    }

    pub async fn load_file(&self, path: String) -> Result<(), String> {
        self.sender
            .send(OggPlayerMessage::Play { path })
            .await
            .map_err(|x| x.to_string())?;

        Ok(())
    }

    pub async fn add_listener(&self, listener: mpsc::Sender<OggPlayerEvent>) -> Result<(), String> {
        self.sender
            .send(OggPlayerMessage::AddListener(listener))
            .await
            .map_err(|x| x.to_string())?;

        Ok(())
    }

    pub async fn get_head(&self) -> Option<Vec<u8>> {
        let (tx, rx) = oneshot::channel();

        println!("Getting head........");
        let _ = self.sender
            .send(OggPlayerMessage::GetHead(tx))
            .await;

        rx.await.expect("Task has been killed")
    }
}
