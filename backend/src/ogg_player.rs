use std::{fs::File, io::BufReader, time::{Duration, SystemTime, UNIX_EPOCH}};
use ogg::{reading::PacketReader};
use opus::{Application, Channels, Decoder as OpusDecoder, Encoder as OpusEncoder};

use tokio::{sync::{mpsc, Mutex}, task, time::sleep};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;

fn generate_serial() -> u32 {
    // Generate a consistent serial number (can be random)
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()

        .subsec_nanos()
}

pub enum OggPlayerEvent {
    AudioData {
        raw_opus_data: Vec<u8>,
        granule_position: u64,
    },
}

pub struct OggPlayer {
    // buffer_data: Mutex<Vec<i16>>, // pcm data
    listeners: Mutex<Vec<mpsc::Sender<OggPlayerEvent>>>,
    granule_position: Mutex<u64>,
    active_file_id: Mutex<u64>,
}

impl OggPlayer {
    pub fn new() -> Self {
        Self {
            listeners: Mutex::new(vec![]),
            granule_position: Mutex::new(0),
            active_file_id: Mutex::new(0),
        }
    }

    pub async fn load_file(&self, path: &str) -> Result<(), String> {
        let cloned_path = path.to_string();

        println!("Reading file {}", path);
        // TODO: Locking twice is not ideal
        *self.active_file_id.lock().await += 1;
        let file_id = *self.active_file_id.lock().await;

        let file = task::spawn_blocking(move || {
            File::open(cloned_path.to_string()).map_err(|x| x.to_string())
        }).await.expect("Should spawn_blocking")?;

        let buf_reader = BufReader::new(file);

        let mut packet_reader = PacketReader::new(buf_reader);

        let mut opus_decoder = OpusDecoder::new(SAMPLE_RATE, Channels::Stereo).map_err(|x| format!("Decoder {}", x.to_string()))?;
        let mut decode_buf = vec![0i16; 1920 * CHANNELS];

        let mut opus_encoder = OpusEncoder::new(SAMPLE_RATE, Channels::Stereo, Application::Audio).map_err(|x| x.to_string())?;

        let max_buffer_size = 5 * 48_000 * 2 * (16 / 8);

        while let Some(packet) = packet_reader.read_packet().map_err(|x| x.to_string())? {
            let active_file_id = *self.active_file_id.lock().await;

            println!("Copmaring id {} == {}", active_file_id, file_id);
            if active_file_id != file_id {
                println!("Changed file {}", file_id);
                break;
            }

            if packet.data.starts_with(b"OpusHead") || packet.data.starts_with(b"OpusTags") {

                continue;
            }

            let frame_size = opus_decoder
                .decode(&packet.data, &mut decode_buf, false)
                .map_err(|x| x.to_string())?;

            let duration_ms = (frame_size as f32 / 48_000 as f32 * 1000.0).round() as u64;

            let pcm = &decode_buf[..frame_size * CHANNELS];

            let mut gp = self.granule_position.lock().await;
            *gp += frame_size as u64;

            let absgp = *gp;
            println!("GP: {}", absgp);
            drop(gp);


            /*
            let mut buffer_data = self.buffer_data.lock().await;
            // If the buffer is empty fill it
            if buffer_data.len() < max_buffer_size {
                buffer_data.extend_from_slice(pcm);
            }
            */
            
            let mut encoded = vec![0u8; 4096];
            let encoded_len = opus_encoder.encode(pcm, &mut encoded).map_err(|x| x.to_string())?;

            for listener in self.listeners.lock().await.iter() {
                listener
                    .send(OggPlayerEvent::AudioData {
                        raw_opus_data: encoded[..encoded_len].to_vec(),
                        granule_position: absgp,
                    })
                    .await
                    .map_err(|x| x.to_string())?;
            }

            sleep(Duration::from_millis(duration_ms)).await;
        }
        
        Ok(())
    }

    pub async fn add_listener(&self, listener: mpsc::Sender<OggPlayerEvent>) {
        let mut listeners = self.listeners.lock().await;

        listeners.push(listener);
    }
}
