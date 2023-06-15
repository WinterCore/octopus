use std::time::Duration;

use tokio::{fs, sync::{mpsc, Mutex}, time::sleep};

use crate::ogg::{OggPage, OggParser};

pub enum OggPlayerEvent {
    AudioData {
        data: Vec<u8>,
        timestamp: u64,
    },
}


pub struct OggPlayer {
    ogg_data: Mutex<Option<Vec<OggPage>>>,
    curr_page_idx: Mutex<usize>,
    prev_page_timestamp: Mutex<u64>,
    listeners: Mutex<Vec<mpsc::Sender<OggPlayerEvent>>>,
}

impl OggPlayer {
    pub fn new() -> Self {
        Self {
            ogg_data: Mutex::new(None),
            curr_page_idx: Mutex::new(0),
            prev_page_timestamp: Mutex::new(0),
            listeners: Mutex::new(vec![]),
        }
    }

    pub async fn get_head(&self) -> Option<Vec<u8>> {
        let ogg_data = self.ogg_data.lock().await;

        let bin: Vec<_> = ogg_data
            .as_ref()?
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

    pub async fn get_initial_buffer_data(&self) -> Option<Vec<u8>> {
        let ogg_data = self.ogg_data.lock().await;
        let i = *self.curr_page_idx.lock().await;

        if let Some(ref data) = *ogg_data {
            return Some(data[(i - 4).max(0)..i].iter().map(|x| x.serialize()).flatten().collect());
        }

        None
    }

    pub async fn load_file(&self, path: &str) -> Result<(), String> {
        let data = fs::read(path)
            .await
            .map_err(|x| x.to_string())?;

        let mut ogg_data = self.ogg_data.lock().await;
        *ogg_data = Some(OggParser::new(&data).into_iter().collect());
        drop(ogg_data);

        *self.curr_page_idx.lock().await = 0;
        self.play().await?;

        Ok(())
    }

    pub async fn add_listener(&self, listener: mpsc::Sender<OggPlayerEvent>) {
        let mut listeners = self.listeners.lock().await;
        listeners.push(listener);
    }

    pub async fn play(&self) -> Result<(), String> {
        let buffer_millis = 2000f64;

        loop {
            let i = {
                let mut curr = self.curr_page_idx.lock().await;
                let val = *curr;
                *curr += 1;
                val
            };

            let ogg_data = self.ogg_data.lock().await;
            let data = match ogg_data.as_ref() {
                None => return Err("There's no loaded file to be played!".to_owned()),
                Some(data) => data,
            };

            let page = match data.get(i).cloned() {
                Some(page) => page,
                None => break,
            };
            drop(data);
            drop(ogg_data);

            let page_timestamp = page.granule_position as f64 / 48_000f64 * 1000f64;
            let prev_page_timestamp = *self.prev_page_timestamp.lock().await as f64;

            for listener in self.listeners.lock().await.iter() {
                listener
                    .send(OggPlayerEvent::AudioData {
                        data: page.serialize(),
                        timestamp: prev_page_timestamp as u64,
                    })
                    .await
                    .map_err(|x| x.to_string())?;
            }

            let sleep_duration = page_timestamp - prev_page_timestamp;

            println!("Sleep duration: {} {}", prev_page_timestamp, sleep_duration);
            if sleep_duration > buffer_millis {
                sleep(Duration::from_millis((sleep_duration - buffer_millis) as u64)).await;
                let prev_page_timestamp = (prev_page_timestamp + (sleep_duration - buffer_millis)) as u64;
                *self.prev_page_timestamp.lock().await = prev_page_timestamp;
                println!("Storing {}", prev_page_timestamp);
            }
        }

        Ok(())
    }
}
