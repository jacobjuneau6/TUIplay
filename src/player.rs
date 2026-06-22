use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

// ── Shared player status ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone)]
pub struct PlayerStatus {
    pub state: PlaybackState,
    pub track_index: Option<usize>,
}

// ── Player ────────────────────────────────────────────────────────────────

pub struct Player {
    pub status: Arc<Mutex<PlayerStatus>>,
    /// Total **mono** audio frames sent to the output device so far.
    pub elapsed_samples: Arc<AtomicU64>,
    /// Total frames in the current track (0 if unknown).
    pub total_samples: Arc<AtomicU64>,
    /// Sample rate of the currently-playing track (set in `play()`).
    pub current_sample_rate: u64,
    stream: Option<cpal::Stream>,
    stop_tx: Option<SyncSender<()>>,
    decoder_handle: Option<thread::JoinHandle<()>>,
    /// (track_path, elapsed_seconds) saved on stop so we can resume.
    resume_point: Option<(String, f64)>,
}

impl Player {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(PlayerStatus {
                state: PlaybackState::Stopped,
                track_index: None,
            })),
            elapsed_samples: Arc::new(AtomicU64::new(0)),
            total_samples: Arc::new(AtomicU64::new(0)),
            current_sample_rate: 48000,
            stream: None,
            stop_tx: None,
            decoder_handle: None,
            resume_point: None,
        }
    }

    // ── Public API ────────────────────────────────────────────────────────

    /// Start playing a track, optionally resuming from `start_seconds`.
    pub fn play(
        &mut self,
        path: &str,
        index: usize,
        duration_secs: Option<u64>,
        start_seconds: Option<f64>,
    ) -> Result<()> {
        self.stop_inner();

        // Clear old resume point (we'll set a new one on stop).
        self.resume_point = None;

        let (sample_tx, sample_rx) = mpsc::sync_channel::<f32>(44100 * 4);
        let (stop_tx, stop_rx) = mpsc::sync_channel::<()>(1);

        let is_opus = path.to_lowercase().ends_with(".opus");
        let (sample_rate, channels) = if is_opus {
            detect_opus_params(path).unwrap_or((48000, 2))
        } else {
            detect_symphonia_params(path).unwrap_or((44100, 2))
        };

        // Total samples for UI display.
        let total = duration_secs
            .map(|s| s * sample_rate as u64)
            .unwrap_or(0);
        self.total_samples.store(total, Ordering::Relaxed);

        // Start offset in samples.
        let start_sample = start_seconds
            .map(|s| (s * sample_rate as f64) as u64)
            .unwrap_or(0);

        self.current_sample_rate = sample_rate as u64;
        self.elapsed_samples.store(start_sample, Ordering::Relaxed);

        // Build audio output stream
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("no audio output device"))?;

        let stream_config = cpal::StreamConfig {
            channels: channels as cpal::ChannelCount,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let elapsed = self.elapsed_samples.clone();
        let n_channels = channels as u64;

        let stream = device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for sample in data.iter_mut() {
                    *sample = sample_rx.try_recv().unwrap_or(0.0);
                }
                // data.len() = total f32 values; divide by channels → audio frames
                let frames = data.len() as u64 / n_channels;
                elapsed.fetch_add(frames, Ordering::Relaxed);
            },
            |err| eprintln!("audio: {}", err),
            None,
        )?;

        stream.play()?;

        // Spawn decoder thread
        let status = self.status.clone();
        let path_owned = path.to_string();
        let dec_elapsed = self.elapsed_samples.clone();

        let handle = thread::spawn(move || {
            let result = if is_opus {
                decode_opus_thread(&path_owned, &sample_tx, &stop_rx, start_sample)
            } else {
                decode_symphonia_thread(&path_owned, &sample_tx, &stop_rx, start_sample)
            };

            if let Err(e) = result {
                eprintln!("decode error: {}", e);
            }

            if let Ok(mut s) = status.lock() {
                s.state = PlaybackState::Stopped;
                s.track_index = None;
            }
            // Reset elapsed counter when finished naturally.
            dec_elapsed.store(0, Ordering::Relaxed);
        });

        {
            let mut s = self.status.lock().unwrap();
            s.state = PlaybackState::Playing;
            s.track_index = Some(index);
        }

        self.stream = Some(stream);
        self.stop_tx = Some(stop_tx);
        self.decoder_handle = Some(handle);

        Ok(())
    }

    pub fn pause(&self) {
        if let Some(ref stream) = self.stream {
            let _ = stream.pause();
            if let Ok(mut s) = self.status.lock() {
                s.state = PlaybackState::Paused;
            }
        }
    }

    pub fn resume(&self) {
        if let Some(ref stream) = self.stream {
            let _ = stream.play();
            if let Ok(mut s) = self.status.lock() {
                s.state = PlaybackState::Playing;
            }
        }
    }

    fn stop_inner(&mut self) {
        if let Some(ref tx) = self.stop_tx {
            let _ = tx.send(());
        }
        self.stream.take();
        self.stop_tx.take();
        if let Some(handle) = self.decoder_handle.take() {
            let _ = handle.join();
        }
    }

    /// Stop playback and remember the current position for later resume.
    pub fn stop_and_remember(&mut self, path: &str) {
        let elapsed = self.elapsed_samples.load(Ordering::Relaxed);
        let secs = elapsed as f64 / self.current_sample_rate as f64;
        self.resume_point = Some((path.to_string(), secs));

        self.stop_inner();
        if let Ok(mut s) = self.status.lock() {
            s.state = PlaybackState::Stopped;
            s.track_index = None;
        }
        self.elapsed_samples.store(0, Ordering::Relaxed);
    }

    pub fn stop(&mut self) {
        self.resume_point = None;
        self.stop_inner();
        if let Ok(mut s) = self.status.lock() {
            s.state = PlaybackState::Stopped;
            s.track_index = None;
        }
        self.elapsed_samples.store(0, Ordering::Relaxed);
    }

    /// Return a saved resume position for `path`, if any.
    pub fn take_resume(&mut self, path: &str) -> Option<f64> {
        match &self.resume_point {
            Some((p, secs)) if p == path => {
                let s = *secs;
                self.resume_point = None;
                Some(s)
            }
            _ => None,
        }
    }

    pub fn toggle_pause(&self) {
        let state = self.status.lock().unwrap().state.clone();
        match state {
            PlaybackState::Playing => self.pause(),
            PlaybackState::Paused => self.resume(),
            PlaybackState::Stopped => {}
        }
    }

    /// Elapsed seconds (frames ÷ sample rate).
    pub fn elapsed_secs(&self) -> f64 {
        let frames = self.elapsed_samples.load(Ordering::Relaxed);
        frames as f64 / self.current_sample_rate as f64
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop_inner();
    }
}

// ── Format detection helpers ──────────────────────────────────────────────

fn detect_opus_params(path: &str) -> Result<(u32, u16)> {
    let mut file = File::open(path)?;
    let mut reader = ogg::reading::PacketReader::new(&mut file);

    let head = reader
        .read_packet()
        .map_err(|e| anyhow!("ogg read error: {}", e))?
        .ok_or_else(|| anyhow!("empty ogg file"))?;

    if head.data.len() < 19 || &head.data[..8] != b"OpusHead" {
        return Err(anyhow!("not a valid Opus stream"));
    }

    let channels = head.data[9] as u16;
    Ok((48000, channels))
}

fn detect_symphonia_params(path: &str) -> Result<(u32, u16)> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let probed = symphonia::default::get_probe().format(
        &Hint::new(),
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("no track in file"))?;
    let rate = track.codec_params.sample_rate.unwrap_or(44100);
    let ch = track
        .codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(2);
    Ok((rate, ch))
}

// ── Ogg-page seek helper ──────────────────────────────────────────────────

/// Seek `file` to the first Ogg page whose granule position is ≥ `target_sample`.
/// Returns the granule position of that page (used by caller to trim).
fn seek_ogg_to_sample(file: &mut File, target_sample: u64) -> Result<(u64, Vec<u8>)> {
    // After seeking, the caller will resume reading packets from the new position.
    // We return `(granule_of_page, leftover_page_data)` so the caller can
    // continue reading from the page data we already consumed during seeking.
    // In practice we just reposition the file and the decoder restarts.
    file.seek(SeekFrom::Start(0))?;
    let mut last_granule = 0u64;
    let mut page_data_leftover: Vec<u8> = Vec::new();
    let _ = &mut page_data_leftover; // reserved for future use

    loop {
        let mut magic = [0u8; 4];
        if file.read_exact(&mut magic).is_err() {
            break;
        }
        if &magic != b"OggS" {
            break;
        }
        let mut hdr = [0u8; 23];
        file.read_exact(&mut hdr)?;
        let gp = u64::from_le_bytes([
            hdr[2], hdr[3], hdr[4], hdr[5], hdr[6], hdr[7], hdr[8], hdr[9],
        ]);
        let n_segs = hdr[22] as usize;
        let mut segs = vec![0u8; n_segs];
        file.read_exact(&mut segs)?;
        let page_bytes: usize = segs.iter().map(|&s| s as usize).sum();

        if gp >= target_sample && gp > 0 {
            // We'll start decoding from this page.  The page data follows
            // immediately; the ogg PacketReader will read it.
            // Reposition to just before the page data.
            let data_start = file.stream_position()?;
            file.seek(SeekFrom::Start(data_start))?; // keep position at page data
            last_granule = gp;
            break;
        }

        // Skip page data
        file.seek(SeekFrom::Current(page_bytes as i64))?;
        last_granule = gp;
    }

    Ok((last_granule, page_data_leftover))
}

// ── Opus decoder (ogg container → opus codec) ─────────────────────────────

fn decode_opus_thread(
    path: &str,
    tx: &SyncSender<f32>,
    stop_rx: &Receiver<()>,
    start_sample: u64,
) -> Result<()> {
    let mut file = File::open(path)?;

    // If seeking, skip to the right Ogg page.
    if start_sample > 0 {
        let _ = seek_ogg_to_sample(&mut file, start_sample);
    }

    let mut reader = ogg::reading::PacketReader::new(&mut file);

    // OpusHead
    let head = reader
        .read_packet()
        .map_err(|e| anyhow!("ogg: {}", e))?
        .ok_or_else(|| anyhow!("empty ogg"))?;
    if head.data.len() < 19 || &head.data[..8] != b"OpusHead" {
        return Err(anyhow!("missing OpusHead"));
    }
    let channels = head.data[9] as u16;
    let _pre_skip = u16::from_le_bytes([head.data[10], head.data[11]]) as u64;

    let opus_channels = match channels {
        1 => opus::Channels::Mono,
        _ => opus::Channels::Stereo,
    };
    let mut decoder = opus::Decoder::new(48000, opus_channels)?;

    // OpusTags – skip
    reader
        .read_packet()
        .map_err(|e| anyhow!("ogg tags: {}", e))?;

    let max_frame_samples = 120 * 48 * channels as usize;
    let mut out = vec![0.0f32; max_frame_samples];

    // Track how many samples we've output so we can trim the first partial frame.
    let mut samples_output: u64 = 0;

    loop {
        if stop_rx.try_recv().is_ok() {
            return Ok(());
        }

        let pkt = match reader.read_packet() {
            Ok(Some(p)) => p,
            Ok(None) => return Ok(()),
            Err(e) => {
                eprintln!("ogg read: {}", e);
                return Ok(());
            }
        };

        match decoder.decode_float(&pkt.data, &mut out, false) {
            Ok(samples) => {
                let total = samples * channels as usize;

                // If we sought, trim samples before the target.
                let trim = if start_sample > samples_output {
                    ((start_sample - samples_output) as usize).min(total)
                } else {
                    0
                };

                for &s in &out[trim..total] {
                    if tx.send(s).is_err() {
                        return Ok(());
                    }
                }
                samples_output += total as u64;
            }
            Err(e) => eprintln!("opus decode: {}", e),
        }
    }
}

// ── Symphonia decoder (mp3, flac, wav, aac, vorbis, …) ────────────────────

fn decode_symphonia_thread(
    path: &str,
    tx: &SyncSender<f32>,
    stop_rx: &Receiver<()>,
    start_sample: u64,
) -> Result<()> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let probed = symphonia::default::get_probe().format(
        &Hint::new(),
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("no track"))?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();

    // Seek if needed (symphonia's FormatReader supports seeking).
    if start_sample > 0 {
        let rate = codec_params.sample_rate.unwrap_or(44100) as u64;
        let seek_ts = symphonia::core::units::Time::new(start_sample, rate as f64);
        // Try seeking; ignore errors — we'll just decode from the start.
        let _ = format.seek(
            symphonia::core::formats::SeekMode::Accurate,
            symphonia::core::formats::SeekTo::Time {
                time: seek_ts,
                track_id: Some(track_id),
            },
        );
    }

    let mut decoder = symphonia::default::get_codecs().make(
        &codec_params,
        &DecoderOptions::default(),
    )?;

    loop {
        if stop_rx.try_recv().is_ok() {
            return Ok(());
        }

        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(e) => {
                let msg = format!("{}", e);
                if msg.contains("end of file") || msg.contains("end of stream") {
                    return Ok(());
                }
                eprintln!("symphonia read: {}", e);
                return Ok(());
            }
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("symphonia decode: {}", e);
                continue;
            }
        };

        let spec = *decoded.spec();
        let duration = decoded.capacity() as u64;
        let mut buf = SampleBuffer::<f32>::new(duration, spec);
        buf.copy_interleaved_ref(decoded);

        for &s in buf.samples() {
            if tx.send(s).is_err() {
                return Ok(());
            }
        }
    }
}
