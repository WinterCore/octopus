use std::{fs::File, io::BufReader, sync::Arc, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use ogg::{reading::PacketReader};
use opus::{Application, Channels, Decoder as OpusDecoder, Encoder as OpusEncoder};

use tokio::{sync::{mpsc, Mutex, Notify}, task, time::sleep};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

const BUFFER_SIZE_MS: usize = 3000; // 5 seconds buffer
const MAX_HEADSTART_BUFFER_SIZE: usize = ((BUFFER_SIZE_MS as f32 / 1000f32) * SAMPLE_RATE as f32 * CHANNELS as f32) as usize;

pub const OPUS_HEAD: &[u8] = &[
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

pub const OPUS_COMMENTS: &[u8] = &[
    // "OpusTags" magic signature
    0x4F, 0x70, 0x75, 0x73, 0x54, 0x61, 0x67, 0x73, // "OpusTags"

    // Vendor string length (7 bytes: "Octopus")
    0x07, 0x00, 0x00, 0x00,

    // Vendor string bytes: "Octopus"
    0x4F, 0x63, 0x74, 0x6F, 0x70, 0x75, 0x73,

    // User comment list length = 0
    0x00, 0x00, 0x00, 0x00,
];


fn generate_serial() -> u32 {
    // Generate a consistent serial number (can be random)
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()

        .subsec_nanos()
}

#[derive(Debug)]
pub enum OpusPlayerEvent {
    AudioData {
        raw_opus_data: Vec<u8>,
        granule_position: u64,
    },
}

#[derive(Debug, Clone)]
pub struct ActiveFile {
    pub id: u64,
    pub start_granule_position: u64,
    pub name: String,
    pub author: String,
    pub image: Option<String>,
    pub duration_ms: u64,
}

pub struct OpusPlayer {
    start_instant: Mutex<Option<Instant>>,
    buffer_data: Mutex<Vec<i16>>, // pcm data
    listeners: Mutex<Vec<mpsc::Sender<OpusPlayerEvent>>>,
    granule_position: Mutex<u64>,
    active_file: Mutex<ActiveFile>,
}

impl OpusPlayer {
    pub fn new() -> Self {
        Self {
            start_instant: Mutex::new(None),
            buffer_data: Mutex::new(Vec::new()),
            listeners: Mutex::new(vec![]),
            granule_position: Mutex::new(0),
            active_file: Mutex::new(ActiveFile {
                id: 0,
                start_granule_position: 0,
                name: "Unknown".to_string(),
                author: "Unknown Author".to_string(),
                image: None,
                duration_ms: 0,
            }),
        }
    }
    
    pub async fn get_metadata(&self) -> ActiveFile {
        let active_file = self.active_file.lock().await;

        return active_file.clone();
    }

    pub async fn get_current_file_start_time_ms(&self) -> u64 {
        let active_file = self.active_file.lock().await;

        let now_playing_ms = active_file.start_granule_position as f64 / 48_000 as f64 * 1000.0;

        return now_playing_ms as u64;
    }

    fn get_file_duration(path: &str) -> Result<u64, String> {
        let file = File::open(path).map_err(|x| x.to_string())?;
        let buf_reader = BufReader::new(file);
        let mut packet_reader = PacketReader::new(buf_reader);

        let mut last_granule_position = 0u64;

        // Read through all packets to find the last granule position
        while let Some(packet) = packet_reader.read_packet().map_err(|x| x.to_string())? {
            if packet.absgp_page() > 0 {
                last_granule_position = packet.absgp_page();
            }
        }

        // Convert granule position to milliseconds
        let duration_ms = (last_granule_position as f64 / SAMPLE_RATE as f64 * 1000.0) as u64;

        Ok(duration_ms)
    }

    pub async fn play_file(
        &self,
        path: &str,
    ) -> Result<(), String> {
        let cloned_path = path.to_string();

        println!("Playing file {}", path);

        // Calculate file duration
        let duration_ms = Self::get_file_duration(path).unwrap_or(0);
        println!("File duration: {:.2} seconds ({} ms)", duration_ms as f64 / 1000.0, duration_ms);

        // TODO: Locking twice is not ideal
        let mut active_file = self.active_file.lock().await;
        active_file.id += 1;
        active_file.start_granule_position = *self.granule_position.lock().await;
        active_file.duration_ms = duration_ms;

        {
            let mut instant = self.start_instant.lock().await;
            if let None = instant.as_ref() {
                *instant = Some(Instant::now());
            }
        }

        let file_id = active_file.id;
        drop(active_file);

        let file = task::spawn_blocking(move || {
            File::open(cloned_path.to_string()).map_err(|x| x.to_string())
        }).await.expect("Should spawn_blocking")?;

        let buf_reader = BufReader::new(file);

        let mut packet_reader = PacketReader::new(buf_reader);

        let mut opus_decoder = OpusDecoder::new(SAMPLE_RATE, Channels::Stereo).map_err(|x| format!("Decoder {}", x.to_string()))?;
        let mut decode_buf = vec![0i16; 1920 * CHANNELS];

        let mut opus_encoder = OpusEncoder::new(SAMPLE_RATE, Channels::Stereo, Application::Audio).map_err(|x| x.to_string())?;

        while let Some(packet) = packet_reader.read_packet().map_err(|x| x.to_string())? {
            let active_file = self.active_file.lock().await;

            if active_file.id != file_id {
                println!("Changed file {}", file_id);
                return Err("Interrupted".to_string());
            }
            drop(active_file);

            if packet.data.starts_with(b"OpusHead") || packet.data.starts_with(b"OpusTags") {

                continue;
            }

            // Frame size
            let frame_size = opus_decoder
                .decode(&packet.data, &mut decode_buf, false)
                .map_err(|x| x.to_string())?;

            let frame_duration_ms = frame_size as f64 / 48_000 as f64 * 1000.0;

            let pcm = &decode_buf[..frame_size * CHANNELS];
            // println!("FRAME_SIZE {}", frame_size);
            // println!("PCM LEN: {}", frame_size * CHANNELS);

            let mut gp = self.granule_position.lock().await;
            *gp += frame_size as u64;

            let absgp = *gp;
            drop(gp);

            let now_playing_ms = absgp as f64 / 48_000 as f64 * 1000.0;

            // println!("GP: {}", absgp);
            // println!("FRAME_SIZE {:?}", frame_size);

            let mut encoded = vec![0u8; 4096];
            let encoded_len = opus_encoder.encode(pcm, &mut encoded).map_err(|x| x.to_string())?;

            let mut listener_indices_to_drop = Vec::new();

            for (i, listener) in self.listeners.lock().await.iter().enumerate() {
                let send_result = listener
                    .try_send(OpusPlayerEvent::AudioData {
                        raw_opus_data: encoded[..encoded_len].to_vec(),
                        granule_position: absgp,
                    });

                if let Err(_) = send_result {
                    println!("Send to listener {} failed. Dropping listener...", i);


                    listener_indices_to_drop.push(i);
                }
            }

            if !listener_indices_to_drop.is_empty() {
                let mut listeners = self.listeners.lock().await;
                
                for i in listener_indices_to_drop.into_iter() {
                    if listeners.len() > i {
                        listeners.remove(i);
                    }
                }
            }

            let mut buffer_data = self.buffer_data.lock().await;

            // If the buffer is empty fill it
            if buffer_data.len() < MAX_HEADSTART_BUFFER_SIZE {
                buffer_data.extend_from_slice(pcm);
            } else {
                // Do a sliding window

                // Remove from start
                buffer_data.drain(0..(frame_size * 2));

                // Add to the end
                buffer_data.extend_from_slice(pcm);

                let instant_option = self.start_instant.lock().await;


                if let Some(instant) = instant_option.as_ref() {
                    let lag_ms = now_playing_ms as i64 - (instant.elapsed().as_millis() as i64 + BUFFER_SIZE_MS as i64);
                    sleep(Duration::from_millis(lag_ms.max(0) as u64)).await;
                } else {
                    sleep(Duration::from_millis(frame_duration_ms as u64)).await;
                }
            }
        }


        Ok(())
    }

    pub async fn get_headstart_data(&self) -> Vec<OpusPlayerEvent> {
        let frame_size_ms = 20f32;
        let frame_size = (SAMPLE_RATE as f32 / (1000f32 / frame_size_ms) * 2f32) as usize;
        let packets = (BUFFER_SIZE_MS as f32 / frame_size_ms).ceil() as usize;
        let mut events: Vec<OpusPlayerEvent> = Vec::with_capacity(packets as usize);
        let mut opus_encoder = OpusEncoder::new(SAMPLE_RATE, Channels::Stereo, Application::Audio)
            .map_err(|x| x.to_string())
            .expect("Should create opus encoder");

        let buffer_data = self.buffer_data.lock().await;

        if buffer_data.is_empty() {
            return Vec::new();
        }

        let mut temp_buffer = vec![0u8; 4096];

        for i in 0..packets {
            let samples: usize = frame_size;

            let encoded_len = opus_encoder
                .encode(&buffer_data[(i * samples)..(i * samples + samples)], &mut temp_buffer)
                .expect("Should encode headstart pcm data");

            let event = OpusPlayerEvent::AudioData {
                raw_opus_data: temp_buffer[..encoded_len].to_vec(),
                granule_position: (i as u64 * samples as u64) / 2,
            };

            events.push(event);
        }

        return events;
    }

    pub async fn add_listener(&self, listener: mpsc::Sender<OpusPlayerEvent>) {
        let mut listeners = self.listeners.lock().await;

        listeners.push(listener);
    }
}
