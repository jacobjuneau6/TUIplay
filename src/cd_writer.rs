//! Export a queue/playlist as a burnable CD image:
//!   1. Decode every track to 44.1 kHz / 16-bit / stereo PCM.
//!   2. Write one `.wav` file per track.
//!   3. Generate a `.cue` sheet so any CD-burning tool can produce a
//!      Red Book audio CD playable in older car stereos.
//!   4. Optionally burn directly to a CD-R using `cdrdao` or `wodim`.

use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Result};

/// Where the exported files land (a sub-directory inside the music folder).
pub fn export_playlist(
    tracks: &[(String, String)], // (path, display_name)
    playlist_name: &str,
    music_dir: &Path,
) -> Result<PathBuf> {
    let out_dir = music_dir.join(format!("cd_{}", sanitise_filename(playlist_name)));
    fs::create_dir_all(&out_dir)?;

    let mut wav_paths: Vec<(PathBuf, u64)> = Vec::new(); // (path, samples_written)

    for (i, (file_path, _display_name)) in tracks.iter().enumerate() {
        let wav_name = format!("track{:02}.wav", i + 1);
        let wav_path = out_dir.join(&wav_name);
        let samples = decode_to_stereo_44100(file_path)?;
        write_wav(&wav_path, &samples)?;
        wav_paths.push((wav_name.into(), samples.len() as u64));
    }

    // Generate CUE sheet
    let cue_path = out_dir.join("disc.cue");
    write_cue(&cue_path, &wav_paths)?;

    Ok(out_dir)
}

// ── Decode to 44.1 kHz stereo 16-bit PCM ──────────────────────────────────

fn decode_to_stereo_44100(path: &str) -> Result<Vec<i16>> {
    let is_opus = path.to_lowercase().ends_with(".opus");

    // Decode to f32 samples at native rate
    let (samples_f32, rate, channels) = if is_opus {
        decode_opus_to_f32(path)?
    } else {
        decode_symphonia_to_f32(path)?
    };

    // Resample to 44100 Hz
    let resampled = if rate != 44100 {
        resample_linear(&samples_f32, channels, rate, 44100)
    } else {
        samples_f32
    };

    // Convert to stereo (duplicate mono, take first 2 of multi)
    let stereo = match channels {
        1 => mono_to_stereo(&resampled),
        2 => resampled,
        _ => extract_stereo(&resampled, channels),
    };

    // Convert f32 → i16, clamping
    Ok(stereo
        .iter()
        .map(|&s| {
            let clamped = s.max(-1.0).min(1.0);
            (clamped * 32767.0) as i16
        })
        .collect())
}

// ── Symphonia decoder ─────────────────────────────────────────────────────

fn decode_symphonia_to_f32(path: &str) -> Result<(Vec<f32>, u32, u16)> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

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
    let rate = track.codec_params.sample_rate.unwrap_or(44100);
    let ch = track
        .codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(2);

    let mut decoder = symphonia::default::get_codecs().make(
        &track.codec_params,
        &DecoderOptions::default(),
    )?;

    let mut all_samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let spec = *decoded.spec();
        let dur = decoded.capacity() as u64;
        let mut buf = SampleBuffer::<f32>::new(dur, spec);
        buf.copy_interleaved_ref(decoded);
        all_samples.extend_from_slice(buf.samples());
    }

    Ok((all_samples, rate, ch))
}

// ── Opus decoder ──────────────────────────────────────────────────────────

fn decode_opus_to_f32(path: &str) -> Result<(Vec<f32>, u32, u16)> {
    let mut file = File::open(path)?;
    let mut reader = ogg::reading::PacketReader::new(&mut file);

    // OpusHead
    let head = reader
        .read_packet()
        .map_err(|e| anyhow!("ogg: {}", e))?
        .ok_or_else(|| anyhow!("empty ogg"))?;
    let channels = head.data[9] as u16;

    let opus_ch = match channels {
        1 => opus::Channels::Mono,
        _ => opus::Channels::Stereo,
    };
    let mut decoder = opus::Decoder::new(48000, opus_ch)?;

    // OpusTags – skip
    reader
        .read_packet()
        .map_err(|e| anyhow!("ogg tags: {}", e))?;

    let max_frame = 120 * 48 * channels as usize;
    let mut out = vec![0.0f32; max_frame];
    let mut all_samples: Vec<f32> = Vec::new();

    loop {
        let pkt = match reader.read_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(e) => {
                eprintln!("ogg: {}", e);
                break;
            }
        };
        match decoder.decode_float(&pkt.data, &mut out, false) {
            Ok(samples) => {
                let total = samples * channels as usize;
                all_samples.extend_from_slice(&out[..total]);
            }
            Err(e) => eprintln!("opus: {}", e),
        }
    }

    Ok((all_samples, 48000, channels))
}

// ── Resampling (linear interpolation) ─────────────────────────────────────

fn resample_linear(samples: &[f32], channels: u16, src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate {
        return samples.to_vec();
    }
    let ratio = src_rate as f64 / dst_rate as f64;
    let total_frames = samples.len() / channels as usize;
    let out_frames = (total_frames as f64 / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_frames * channels as usize);

    for out_frame in 0..out_frames {
        let src_pos = out_frame as f64 * ratio;
        let src_frame = src_pos as usize;
        let frac = src_pos - src_frame as f64;
        let next_frame = (src_frame + 1).min(total_frames - 1);

        for ch in 0..channels as usize {
            let a = samples[src_frame * channels as usize + ch];
            let b = samples[next_frame * channels as usize + ch];
            out.push((a as f64 + (b as f64 - a as f64) * frac) as f32);
        }
    }
    out
}

// ── Channel conversion ────────────────────────────────────────────────────

fn mono_to_stereo(mono: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(mono.len() * 2);
    for &s in mono {
        out.push(s);
        out.push(s);
    }
    out
}

fn extract_stereo(multi: &[f32], channels: u16) -> Vec<f32> {
    let frames = multi.len() / channels as usize;
    let mut out = Vec::with_capacity(frames * 2);
    for f in 0..frames {
        out.push(multi[f * channels as usize]);
        out.push(multi[f * channels as usize + 1]);
    }
    out
}

// ── WAV writer ────────────────────────────────────────────────────────────

fn write_wav(path: &Path, samples: &[i16]) -> Result<()> {
    let file = File::create(path)?;
    let mut w = BufWriter::new(file);

    let data_size = (samples.len() * 2) as u32; // 2 bytes per sample
    let file_size = 36 + data_size;

    // RIFF header
    w.write_all(b"RIFF")?;
    w.write_all(&file_size.to_le_bytes())?;
    w.write_all(b"WAVE")?;

    // fmt chunk
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?; // chunk size
    w.write_all(&1u16.to_le_bytes())?; // PCM = 1
    w.write_all(&2u16.to_le_bytes())?; // stereo
    w.write_all(&44100u32.to_le_bytes())?; // sample rate
    w.write_all(&(44100u32 * 4).to_le_bytes())?; // byte rate
    w.write_all(&4u16.to_le_bytes())?; // block align
    w.write_all(&16u16.to_le_bytes())?; // bits per sample

    // data chunk
    w.write_all(b"data")?;
    w.write_all(&data_size.to_le_bytes())?;
    for &s in samples {
        w.write_all(&s.to_le_bytes())?;
    }
    w.flush()?;
    Ok(())
}

// ── CUE sheet writer ──────────────────────────────────────────────────────

fn write_cue(path: &Path, tracks: &[(PathBuf, u64)]) -> Result<()> {
    let mut f = File::create(path)?;
    writeln!(f, "REM Generated by TUIplay")?;
    writeln!(f, "REM Audio CD image — burn with cdrdao, cdrecord, or similar")?;
    writeln!(f)?;

    for (i, (wav_name, _samples)) in tracks.iter().enumerate() {
        writeln!(f, "FILE \"{}\" WAVE", wav_name.display())?;
        writeln!(f, "  TRACK {:02} AUDIO", i + 1)?;
        writeln!(f, "    INDEX 01 00:00:00")?;
    }
    writeln!(f)?;
    writeln!(f, "REM Total tracks: {}", tracks.len())?;
    Ok(())
}

fn sanitise_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// ── Burn to disc ──────────────────────────────────────────────────────────

/// Try to burn `disc.cue` inside `export_dir` to a CD-R.
///
/// Returns a human-readable status message.
pub fn burn_disc(export_dir: &Path, device: Option<&str>) -> Result<String> {
    let cue_path = export_dir.join("disc.cue");

    if !cue_path.exists() {
        return Err(anyhow!("CUE sheet not found at {}", cue_path.display()));
    }

    // Prefer cdrdao; fall back to wodim/cdrecord.
    if which("cdrdao").is_some() {
        let dev = device
            .map(|d| d.to_string())
            .or_else(detect_cd_device)
            .unwrap_or_else(|| "/dev/sr0".to_string());

        let output = Command::new("cdrdao")
            .arg("write")
            .arg("--device")
            .arg(&dev)
            .arg(&cue_path)
            .output()
            .map_err(|e| anyhow!("failed to run cdrdao: {}", e))?;

        if output.status.success() {
            Ok(format!(
                "Burned successfully with cdrdao (device {}) — {} tracks",
                dev,
                "?"
            ))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("cdrdao failed: {}", stderr))
        }
    } else if which("wodim").is_some() || which("cdrecord").is_some() {
        // wodim/cdrecord burn individual WAV files.
        let burn_cmd = if which("wodim").is_some() {
            "wodim"
        } else {
            "cdrecord"
        };
        let dev = device
            .map(|d| d.to_string())
            .or_else(detect_cd_device)
            .unwrap_or_else(|| "/dev/sr0".to_string());

        // Gather WAVs
        let mut wavs: Vec<PathBuf> = Vec::new();
        for entry in fs::read_dir(export_dir)? {
            let entry = entry?;
            let p = entry.path();
            if p.extension().map(|e| e == "wav").unwrap_or(false) {
                wavs.push(p);
            }
        }
        wavs.sort();

        let mut cmd = Command::new(burn_cmd);
        cmd.arg("-v").arg("-dev=".to_string() + &dev).arg("-audio");
        for w in &wavs {
            cmd.arg(w);
        }

        let output = cmd
            .output()
            .map_err(|e| anyhow!("failed to run {}: {}", burn_cmd, e))?;

        if output.status.success() {
            Ok(format!(
                "Burned {} tracks with {} (device {})",
                wavs.len(),
                burn_cmd,
                dev
            ))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("{} failed: {}", burn_cmd, stderr))
        }
    } else {
        Err(anyhow!(
            "No CD-burning tool found.  Install cdrdao or wodim:\n  sudo pacman -S cdrdao"
        ))
    }
}

fn which(cmd: &str) -> Option<PathBuf> {
    Command::new("which")
        .arg(cmd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            PathBuf::from(s)
        })
}

fn detect_cd_device() -> Option<String> {
    // Common Linux device paths
    for dev in &["/dev/sr0", "/dev/cdrom", "/dev/cdrw", "/dev/cdwriter"] {
        if Path::new(dev).exists() {
            return Some(dev.to_string());
        }
    }
    // Try scanning with cdrdao
    if let Ok(output) = Command::new("cdrdao").arg("scanbus").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(pos) = line.find("/dev/") {
                let dev = line[pos..].split_whitespace().next()?;
                return Some(dev.to_string());
            }
        }
    }
    None
}

// ── Copy tracks to folder ─────────────────────────────────────────────────

/// Copy the actual media files for a set of tracks into `dest_dir`.
/// Returns (copied, total).
pub fn copy_tracks_to_folder(
    tracks: &[(String, String)], // (path, display_name)
    dest_dir: &Path,
) -> Result<(usize, usize)> {
    fs::create_dir_all(dest_dir)?;
    let total = tracks.len();
    let mut copied = 0usize;

    for (i, (src, _display)) in tracks.iter().enumerate() {
        let src_path = Path::new(src);
        let ext = src_path.extension().and_then(|e| e.to_str()).unwrap_or("opus");
        let dest_name = format!(
            "{:02} - {}.{}",
            i + 1,
            sanitise_filename(
                src_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("track")
            ),
            ext
        );
        let dest = dest_dir.join(&dest_name);
        if let Err(e) = fs::copy(src_path, &dest) {
            eprintln!("copy failed for {}: {}", src, e);
        } else {
            copied += 1;
        }
    }

    Ok((copied, total))
}
