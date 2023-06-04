use std::{time::Duration, sync::atomic::{AtomicUsize, Ordering, AtomicU64}};

use tokio::{fs, sync::{mpsc, RwLock}, time::sleep};

use crate::ogg::{OggPage, OggParser};


pub struct OggPlayer {
    ogg_data: RwLock<Option<Vec<OggPage>>>,
    curr_page_idx: AtomicUsize,
    prev_page_timestamp: AtomicU64,
    sender: mpsc::Sender<Vec<u8>>,
}

impl OggPlayer {
    pub fn new(sender: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            ogg_data: RwLock::new(None),
            curr_page_idx: AtomicUsize::new(0),
            prev_page_timestamp: AtomicU64::new(0),
            sender,
        }
    }

    pub async fn get_head(&self) -> Option<Vec<u8>> {
        let ogg_data = self.ogg_data
            .read()
            .await;

        let bin: Vec<_> = ogg_data.as_ref()?
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

    pub async fn load_file(&self, path: &str) -> Result<(), String> {
        let data = fs::read(path)
            .await
            .map_err(|x| x.to_string())?;

        let mut ogg_data = self.ogg_data.write().await;
        *ogg_data = Some(OggParser::new(&data).into_iter().collect());
        self.curr_page_idx.store(0, Ordering::Relaxed);

        Ok(())
    }

    pub async fn play(&self) -> Result<(), String> {
        let buffer_millis = 2000f64;

        loop {
            let data_option = self.ogg_data.read().await;

            let i = self.curr_page_idx.fetch_add(1, Ordering::Relaxed);
            let data = match data_option.as_ref() {
                None => return Err("There's no loaded file to be played!".to_owned()),
                Some(data) => data,
            };

            let page = match data.get(i).cloned() {
                Some(page) => page,
                None => break,
            };
            drop(data);

            self.sender
                .send(page.serialize())
                .await
                .map_err(|x| x.to_string())?;

            let page_timestamp = page.granule_position as f64 / 48_000f64 * 1000f64;
            let prev_page_timestamp = self.prev_page_timestamp.load(Ordering::Relaxed) as f64;

            let sleep_duration = page_timestamp - prev_page_timestamp;

            println!("Sleep duration: {} {}", prev_page_timestamp, sleep_duration);
            if sleep_duration > buffer_millis {
                sleep(Duration::from_millis((sleep_duration - buffer_millis) as u64)).await;
                let prev_page_timestamp = (prev_page_timestamp + (sleep_duration - buffer_millis)) as u64;
                println!("Storing {}", prev_page_timestamp);
                self.prev_page_timestamp.store(prev_page_timestamp, Ordering::Relaxed);
            }
        }

        Ok(())
    }
}
