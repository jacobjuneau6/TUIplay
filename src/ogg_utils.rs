//! Helpers for reading Ogg page headers — used for duration detection
//! and sample-accurate seeking within Opus files.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use anyhow::{anyhow, Result};

/// Return the **last** granule position found in an Ogg file.
///
/// For Opus streams the granule position is the total number of PCM samples
/// encoded up to the end of that page.  Dividing by 48 000 gives seconds.
pub fn read_last_granule(path: &str) -> Result<u64> {
    let mut file = File::open(path)?;
    let len = file.seek(SeekFrom::End(0))?;

    // Search the last 128 KiB for the final "OggS" marker.
    let search = 131_072_u64.min(len);
    file.seek(SeekFrom::Start(len - search))?;
    let mut buf = vec![0u8; search as usize];
    file.read_exact(&mut buf)?;

    let last_marker = buf
        .windows(4)
        .enumerate()
        .rev()
        .find(|(_, w)| *w == b"OggS")
        .map(|(i, _)| len - search + i as u64);

    match last_marker {
        Some(off) => Ok(read_granule_at(&mut file, off)?),
        None => {
            // Fallback: scan the entire file (small / single-page files).
            file.seek(SeekFrom::Start(0))?;
            granule_from_scan(&mut file)
        }
    }
}

/// Read the 8-byte little-endian granule position at the given file offset.
fn read_granule_at(file: &mut File, page_offset: u64) -> Result<u64> {
    file.seek(SeekFrom::Start(page_offset + 6))?; // skip "OggS" + version + flags
    let mut b = [0u8; 8];
    file.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

/// Brute-force scan through every Ogg page, returning the largest granule.
fn granule_from_scan(file: &mut File) -> Result<u64> {
    file.seek(SeekFrom::Start(0))?;
    let mut best = 0u64;
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
        // hdr[2..10] = granule_position (LE u64)
        let gp = u64::from_le_bytes([
            hdr[2], hdr[3], hdr[4], hdr[5], hdr[6], hdr[7], hdr[8], hdr[9],
        ]);
        if gp > best {
            best = gp;
        }
        let n_segs = hdr[22] as usize;
        let mut segs = vec![0u8; n_segs];
        file.read_exact(&mut segs)?;
        let skip: i64 = segs.iter().map(|&s| s as i64).sum();
        file.seek(SeekFrom::Current(skip))?;
    }
    if best == 0 {
        Err(anyhow!("no Ogg pages with non-zero granule"))
    } else {
        Ok(best)
    }
}
