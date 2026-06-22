use anyhow::Result;
use rusqlite::Connection;
use walkdir::WalkDir;

use crate::metadata::read_metadata;

pub fn scan_folder(conn: &Connection, root: &str) -> Result<()> {
    for entry in WalkDir::new(root) {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                eprintln!("walkdir error: {}", e);
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        let supported = ["mp3", "flac", "ogg", "opus", "wav", "m4a"];

        if !supported.contains(&ext.as_str()) {
            continue;
        }

        let meta = match read_metadata(path.to_str().unwrap()) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Skipping {}: {}", path.display(), e);
                continue;
            }
        };

        // Duration (seconds), if we can determine it.
        let duration_secs: Option<u64> = if ext == "opus" {
            crate::ogg_utils::read_last_granule(path.to_str().unwrap())
                .ok()
                .map(|samples| samples / 48_000)
        } else {
            // symphonia can give us duration for some containers.
            read_symphonia_duration(path.to_str().unwrap())
        };

        if let Err(e) = conn.execute(
            "
            INSERT OR REPLACE INTO tracks
            (path, title, artist, album, genre, duration)
            VALUES(?1, ?2, ?3, ?4, ?5, ?6)
            ",
            rusqlite::params![
                path.to_string_lossy(),
                meta.title,
                meta.artist,
                meta.album,
                meta.genre,
                duration_secs,
            ],
        ) {
            eprintln!("DB insert error for {}: {}", path.display(), e);
        }
    }

    Ok(())
}

fn read_symphonia_duration(path: &str) -> Option<u64> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;

    let track = probed.format.default_track()?;
    let n_frames = track.codec_params.n_frames?;     // total audio frames
    let rate = track.codec_params.sample_rate.unwrap_or(44100) as u64;
    Some(n_frames / rate) // seconds
}
