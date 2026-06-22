use std::path;

use anyhow::{Ok, Result};
use rusqlite::Connection;
use tokio::fs::read;
use walkdir::WalkDir;

use crate::metadata::read_metadata;

pub fn scan_folder(
    conn: &Connection,
    root: &str,
 ) -> Result<()> {
    for entry in WalkDir::new(root) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        let supported = [
            "mp3",
            "flac",
            "ogg",
            "opus",
            "wav",
            "m4a"
        ];

        if !supported.contains(&ext.as_str()) {
            continue;
        }

        let meta = read_metadata(
            path.to_str().unwrap()
        )?;

        conn.execute(
            "
            INSERT OR REPLACE INTO tracks
            (
                path,
                title,
                artist,
                album,
                genre
            )
            VALUES(?1,?2,?3,?4,?5)
            ",
            (
                path.to_string_lossy(),
                meta.title,
                meta.artist,
                meta.album,
                meta.genre,
            ),
        )?;

    }

    Ok(())
 }
