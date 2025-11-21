use std::{fs::File, io::{BufReader, Seek}, path::Path, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use ogg::{reading::PacketReader};
use opus::{Application, Channels, Decoder as OpusDecoder, Encoder as OpusEncoder};

use tokio::{sync::{mpsc, oneshot}, task, time::sleep};

use crate::oeggs::get_opus_comments;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

pub const BUFFER_SIZE_MS: usize = 3000; // 5 seconds buffer
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

pub struct PlaybackState {
    packet_reader: PacketReader<BufReader<File>>,
    opus_decoder: OpusDecoder,
    opus_encoder: OpusEncoder,
    decode_buf: Vec<i16>,
    file_id: u64,
}

#[derive(Debug)]
pub enum OpusPlayerEvent {
    AudioData {
        raw_opus_data: Vec<u8>,
        granule_position: u64,
    },
}

#[derive(Debug, Clone)]
pub struct TimeData {
    pub start_time_ms: u64,
    pub current_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ActiveFileMetadata {
    pub id: u64,
    pub start_granule_position: u64,
    pub title: String,
    pub author: String,
    pub image: Option<String>,
    pub duration_ms: u64,
}

pub struct OpusPlayer {
    start_instant: Option<Instant>,
    headstart_buffer: Vec<i16>, // pcm data
    headstart_granule_position: u64, // granule position at the start of headstart_buffer
    listeners: Vec<mpsc::Sender<OpusPlayerEvent>>,
    granule_position: u64,
    active_file: Option<ActiveFileMetadata>,
    current_playlist_path: Option<String>,
}

impl OpusPlayer {
    pub fn new() -> Self {
        Self {
            start_instant: None,
            headstart_buffer: Vec::new(),
            headstart_granule_position: 0,
            listeners: vec![],
            granule_position: 0,
            active_file: None,
            current_playlist_path: None,
        }
    }
    
    pub async fn get_metadata(&self) -> Option<ActiveFileMetadata> {
        return self.active_file.clone();
    }

    pub async fn get_playlist_path(&self) -> Option<String> {
        return self.current_playlist_path.clone();
    }

    pub async fn get_stream_time_data(&self) -> TimeData {
        let active_file = match &self.active_file {
            Some(file) => file,
            None => {
                return TimeData {
                    start_time_ms: 0,
                    current_time_ms: 0,
                };
            }
        };

        let start_time_ms = active_file.start_granule_position as f64 / 48_000 as f64 * 1000.0;
        let current_time_ms = self.granule_position as f64 / 48_000 as f64 * 1000.0;

        return TimeData {
            start_time_ms: start_time_ms as u64,
            current_time_ms: (current_time_ms as u64).checked_sub(BUFFER_SIZE_MS as u64).unwrap_or(0),
        };
    }

    fn get_file_duration(file: &File) -> Result<u64, String> {
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

    pub async fn start_playback(
        &mut self,
        path: &str,
    ) -> Result<PlaybackState, String> {
        let cloned_path = path.to_string();

        println!("Playing file {}", path);

        // Extract and store playlist path (directory containing the file)
        if let Some(parent) = Path::new(path).parent() {
            self.current_playlist_path = Some(parent.to_string_lossy().to_string());
        }

        if let None = self.start_instant {
            self.start_instant = Some(Instant::now());
        }

        let mut file = task::spawn_blocking(move || {
            File::open(cloned_path.to_string()).map_err(|x| x.to_string())
        }).await.expect("Should spawn_blocking")?;

        let duration_ms = Self::get_file_duration(&file).unwrap_or(0);
        file.seek(std::io::SeekFrom::Start(0)).map_err(|x| x.to_string())?;

        let ogg_comments_result = get_opus_comments(&mut file);
        file.seek(std::io::SeekFrom::Start(0)).map_err(|x| x.to_string())?;

        let (title, author) = match ogg_comments_result {
            Ok(comments) => {
                (
                    comments.title().unwrap_or("Unknown Title").to_string(),
                    comments.artist().unwrap_or("Unknown Author").to_string()
                )
            },
            Err(e) => {
                println!("Failed to read Ogg comments: {}", e);

                (
                    "Unknown Title".to_string(),
                    "Unknown Author".to_string()
                )
            }
        };
        
        let file_id = match &self.active_file {
            Some(file) => file.id + 1,
            None => 1,
        };

        self.active_file = Some(ActiveFileMetadata {
            id: file_id,
            start_granule_position: self.granule_position,
            title,
            author,
            image: Some("/playlist-image".to_string()),
            duration_ms,
        });

        // Calculate file duration
        println!("\tFile duration: {:.2} seconds ({} ms)", duration_ms as f64 / 1000.0, duration_ms);

        let buf_reader = BufReader::new(file);
        let packet_reader = PacketReader::new(buf_reader);

        let opus_decoder = OpusDecoder::new(SAMPLE_RATE, Channels::Stereo)
            .map_err(|x| format!("Decoder {}", x.to_string()))?;
        let decode_buf = vec![0i16; 1920 * CHANNELS];

        let opus_encoder = OpusEncoder::new(SAMPLE_RATE, Channels::Stereo, Application::Audio)
            .map_err(|x| x.to_string())?;

        Ok(PlaybackState {
            packet_reader,
            opus_decoder,
            opus_encoder,
            decode_buf,
            file_id,
        })
    }

    pub async fn process_next_packet(
        &mut self,
        state: &mut PlaybackState,
    ) -> Result<bool, String> {
        let active_file = match &self.active_file {
            Some(file) => file,
            None => return Err("No active file for playback".to_string()),
        };

        // Try to read the next packet
        let packet = match state.packet_reader.read_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => return Ok(false), // EOF - no more packets
            Err(e) => return Err(e.to_string()),
        };

        // Check if playback was interrupted by a new file
        if active_file.id != state.file_id {
            return Err("Playback interrupted: File changed!".to_string());
        }

        // Skip header packets
        if packet.data.starts_with(b"OpusHead") || packet.data.starts_with(b"OpusTags") {
            return Ok(true); // Continue to next packet
        }

        // Decode the packet
        let frame_size = state.opus_decoder
            .decode(&packet.data, &mut state.decode_buf, false)
            .map_err(|x| x.to_string())?;

        let frame_duration_ms = frame_size as f64 / 48_000 as f64 * 1000.0;
        let pcm = &state.decode_buf[..frame_size * CHANNELS];

        // Update granule position
        self.granule_position += frame_size as u64;
        let absgp = self.granule_position;
        let now_playing_ms = absgp as f64 / 48_000 as f64 * 1000.0;

        // Re-encode the audio
        let mut encoded = vec![0u8; 4096];
        let encoded_len = state.opus_encoder.encode(pcm, &mut encoded)
            .map_err(|x| x.to_string())?;

        // Broadcast to all listeners
        let mut listener_indices_to_drop = Vec::new();

        for (i, listener) in self.listeners.iter().enumerate() {
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

        // Remove disconnected listeners
        if !listener_indices_to_drop.is_empty() {
            for i in listener_indices_to_drop.into_iter().rev() {
                if self.listeners.len() > i {
                    self.listeners.remove(i);
                }
            }
        }

        // Update buffer with sliding window
        let buffer_data = &mut self.headstart_buffer;

        if buffer_data.len() < MAX_HEADSTART_BUFFER_SIZE {
            // Fill initial buffer
            if buffer_data.is_empty() {
                // First frame being added - set headstart position
                self.headstart_granule_position = self.granule_position - frame_size as u64;
            }
            buffer_data.extend_from_slice(pcm);
        } else {
            // Sliding window - remove old data, add new data
            let samples_drained = frame_size; // Number of samples per channel
            buffer_data.drain(0..(frame_size * 2));
            buffer_data.extend_from_slice(pcm);

            // Update headstart position to reflect removed samples
            self.headstart_granule_position += samples_drained as u64;

            // Sleep to maintain real-time playback speed
            if let Some(instant) = self.start_instant {
                let lag_ms = now_playing_ms as i64 - (instant.elapsed().as_millis() as i64 + BUFFER_SIZE_MS as i64);
                sleep(Duration::from_millis(lag_ms.max(0) as u64)).await;
            } else {
                sleep(Duration::from_millis(frame_duration_ms as u64)).await;
            }
        }

        Ok(true) // More packets remain
    }

    pub async fn play_file(
        &mut self,
        path: &str,
    ) -> Result<(), String> {
        // Initialize playback
        let mut state = self.start_playback(path).await?;

        // Process packets until done
        loop {
            match self.process_next_packet(&mut state).await {
                Ok(true) => continue,  // More packets to process
                Ok(false) => return Ok(()), // EOF - done successfully
                Err(e) => return Err(e), // Error during playback
            }
        }
    }

    pub async fn get_headstart_data(&self) -> Vec<OpusPlayerEvent> {
        let frame_size_ms = 20f32;
        let frame_size = (SAMPLE_RATE as f32 / (1000f32 / frame_size_ms) * 2f32) as usize;
        let packets = (BUFFER_SIZE_MS as f32 / frame_size_ms).ceil() as usize;
        let mut events: Vec<OpusPlayerEvent> = Vec::with_capacity(packets as usize);
        let mut opus_encoder = OpusEncoder::new(SAMPLE_RATE, Channels::Stereo, Application::Audio)
            .map_err(|x| x.to_string())
            .expect("Should create opus encoder");

        if self.headstart_buffer.is_empty() {
            return Vec::new();
        }

        let mut temp_buffer = vec![0u8; 4096];

        for i in 0..packets {
            let samples: usize = frame_size;

            let encoded_len = opus_encoder
                .encode(&self.headstart_buffer[(i * samples)..(i * samples + samples)], &mut temp_buffer)
                .expect("Should encode headstart pcm data");

            let event = OpusPlayerEvent::AudioData {
                raw_opus_data: temp_buffer[..encoded_len].to_vec(),
                granule_position: self.granule_position + (i as u64 * samples as u64) / 2,
            };

            events.push(event);
        }

        return events;
    }

    pub async fn add_listener(&mut self, listener: mpsc::Sender<OpusPlayerEvent>) {
        self.listeners.push(listener);
    }
}

enum OpusPlayerCommand {
    PlayFile(oneshot::Sender<PlaybackResult>, String),
    GetMetadata(oneshot::Sender<Option<ActiveFileMetadata>>),
    GetHeadstartData(oneshot::Sender<Vec<OpusPlayerEvent>>),
    GetTimeData(oneshot::Sender<TimeData>),
    RegisterListener(mpsc::Sender<OpusPlayerEvent>),
    GetPlaylistPath(oneshot::Sender<Option<String>>),
}

pub enum PlaybackResult {
    Finished,
    Interrupted,
    Error(String),
}

struct OpusPlayerActor {
    player: OpusPlayer,
    receiver: tokio::sync::mpsc::Receiver<OpusPlayerCommand>,
}

impl OpusPlayerActor {
    pub fn new(receiver: tokio::sync::mpsc::Receiver<OpusPlayerCommand>) -> Self {
        Self {
            player: OpusPlayer::new(),
            receiver,
        }
    }

    pub async fn run(mut self) {
        let mut playback_state: Option<(PlaybackState, oneshot::Sender<PlaybackResult>)> = None;

        loop {
            tokio::select! {
                // Process incoming commands
                Some(command) = self.receiver.recv() => {
                    match command {
                        OpusPlayerCommand::PlayFile(sender, path) => {
                            // If already playing, notify the old sender that playback was interrupted
                            if let Some((_, old_sender)) = playback_state.take() {
                                let _ = old_sender.send(PlaybackResult::Interrupted);
                            }

                            // Start new playback
                            match self.player.start_playback(&path).await {
                                Ok(state) => {
                                    playback_state = Some((state, sender));
                                },
                                Err(e) => {
                                    println!("Error starting playback: {}", e);
                                    let _ = sender.send(PlaybackResult::Error(e));
                                }
                            }
                        },
                        OpusPlayerCommand::GetMetadata(sender) => {
                            let metadata = self.player.get_metadata().await;

                            if let Err(e) = sender.send(metadata) {
                                println!("Error sending metadata: {:?}", e);
                            }
                        },
                        OpusPlayerCommand::RegisterListener(listener) => {
                            self.player.add_listener(listener).await;
                        },
                        OpusPlayerCommand::GetHeadstartData(sender) => {
                            let data = self.player.get_headstart_data().await;

                            if let Err(e) = sender.send(data) {
                                println!("Error sending headstart data: {:?}", e);
                            }
                        },
                        OpusPlayerCommand::GetTimeData(sender) => {
                            let time_data = self.player.get_stream_time_data().await;

                            if let Err(e) = sender.send(time_data) {
                                println!("Error sending time data: {:?}", e);
                            }
                        },
                        OpusPlayerCommand::GetPlaylistPath(sender) => {
                            let playlist_path = self.player.get_playlist_path().await;

                            if let Err(e) = sender.send(playlist_path) {
                                println!("Error sending playlist path: {:?}", e);
                            }
                        },
                    }
                }

                // Process next packet if currently playing
                result = async {
                    match playback_state.as_mut() {
                        Some((state, _)) => Some(self.player.process_next_packet(state).await),
                        None => None,
                    }
                }, if playback_state.is_some() => {
                    match result {
                        Some(Ok(true)) => {
                            // Continue playing - more packets remain
                        },
                        Some(Ok(false)) => {
                            // Playback finished successfully
                            if let Some((_, sender)) = playback_state.take() {
                                let _ = sender.send(PlaybackResult::Finished);
                            }
                        },
                        Some(Err(e)) => {
                            // Error during playback
                            println!("Playback error: {}", e);
                            if let Some((_, sender)) = playback_state.take() {
                                let _ = sender.send(PlaybackResult::Error(e.to_string()));
                            }
                        },
                        None => {
                            // Playback was interrupted

                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpusPlayerHandle {
    sender: mpsc::Sender<OpusPlayerCommand>,
}

impl OpusPlayerHandle {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(50);
        let actor = OpusPlayerActor::new(receiver);
        
        // Spawn the actor in its own thread
        tokio::spawn(async move {
            actor.run().await;
        });
        
        Self { sender }
    }

    pub async fn play_file(&self, path: String) -> Result<PlaybackResult, String> {
        let (sender, receiver) = oneshot::channel();

        let command = OpusPlayerCommand::PlayFile(sender, path);

        self.sender.send(command).await.map_err(|x| x.to_string())?;

        let result = receiver.await.map_err(|x| x.to_string())?;

        Ok(result)
    }

    pub async fn get_metadata(&self) -> Result<Option<ActiveFileMetadata>, String> {
        let (sender, receiver) = oneshot::channel();

        let command = OpusPlayerCommand::GetMetadata(sender);

        self.sender.send(command).await.map_err(|x| x.to_string())?;

        let metadata = receiver.await.map_err(|x| x.to_string())?;

        Ok(metadata)
    }

    pub async fn register_listener(&self, listener: mpsc::Sender<OpusPlayerEvent>) -> Result<(), String> {
        let command = OpusPlayerCommand::RegisterListener(listener);

        self.sender.send(command).await.map_err(|x| x.to_string())?;

        Ok(())
    }

    pub async fn get_headstart_data(&self) -> Result<Vec<OpusPlayerEvent>, String> {
        let (sender, receiver) = oneshot::channel();

        let command = OpusPlayerCommand::GetHeadstartData(sender);

        self.sender.send(command).await.map_err(|x| x.to_string())?;

        let data = receiver.await.map_err(|x| x.to_string())?;

        Ok(data)
    }

    pub async fn get_time_data(&self) -> Result<TimeData, String> {
        let (sender, receiver) = oneshot::channel();

        let command = OpusPlayerCommand::GetTimeData(sender);

        self.sender.send(command).await.map_err(|x| x.to_string())?;

        let time_data = receiver.await.map_err(|x| x.to_string())?;

        Ok(time_data)
    }

    pub async fn get_playlist_path(&self) -> Result<Option<String>, String> {
        let (sender, receiver) = oneshot::channel();

        let command = OpusPlayerCommand::GetPlaylistPath(sender);

        self.sender.send(command).await.map_err(|x| x.to_string())?;

        let playlist_path = receiver.await.map_err(|x| x.to_string())?;

        Ok(playlist_path)
    }
}

