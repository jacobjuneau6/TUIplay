use anyhow::Result;
use lofty::file::TaggedFileExt;
use lofty::prelude::ItemKey;
use lofty::tag;

pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
}

pub fn read_metadata(path: &str) -> Result<TrackMeta> {
    let tagged = lofty::read_from_path(path)?;
    let tag = tagged.primary_tag();

    Ok(TrackMeta {
        title: tag
            .and_then(|t| t.get_string(&ItemKey::TrackTitle))
            .unwrap_or("Unknown")
            .to_string(),
        artist: tag
            .and_then(|t| t.get_string(&ItemKey::TrackArtist))
            .unwrap_or("Unknown")
            .to_string(),
        album: tag
            .and_then(|t| t.get_string(&ItemKey::AlbumTitle))
            .unwrap_or("Unknown")
            .to_string(),
        genre: tag
            .and_then(|t| t.get_string(&ItemKey::Genre))
            .unwrap_or("")
            .to_string(),
    })
}