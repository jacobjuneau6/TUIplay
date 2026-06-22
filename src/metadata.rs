use anyhow::Result;
use lofty::file::TaggedFileExt;
use lofty::prelude::ItemKey;

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

pub fn write_metadata(
    path: &str,
    title: &str,
    artist: &str,
    album: &str,
    genre: &str,
) -> Result<()> {
    let mut tagged = lofty::read_from_path(path)?;

    // `lofty` 0.22 uses `insert_text` (not `set_text`).
    let applied = if let Some(tag) = tagged.primary_tag_mut() {
        tag.insert_text(ItemKey::TrackTitle, String::from(title));
        tag.insert_text(ItemKey::TrackArtist, String::from(artist));
        tag.insert_text(ItemKey::AlbumTitle, String::from(album));
        tag.insert_text(ItemKey::Genre, String::from(genre));
        true
    } else {
        // Fallback: try to find any writable tag type.
        // Opus, Vorbis, MP3, FLAC all have primary tags so we rarely reach here.
        let tag_type = tagged
            .supports_tag_type(lofty::tag::TagType::VorbisComments)
            .then_some(lofty::tag::TagType::VorbisComments)
            .or_else(|| {
                tagged
                    .supports_tag_type(lofty::tag::TagType::Id3v2)
                    .then_some(lofty::tag::TagType::Id3v2)
            })
            .or_else(|| {
                tagged
                    .supports_tag_type(lofty::tag::TagType::Ape)
                    .then_some(lofty::tag::TagType::Ape)
            });

        if let Some(tt) = tag_type {
            if let Some(tag) = tagged.tag_mut(tt) {
                tag.insert_text(ItemKey::TrackTitle, String::from(title));
                tag.insert_text(ItemKey::TrackArtist, String::from(artist));
                tag.insert_text(ItemKey::AlbumTitle, String::from(album));
                tag.insert_text(ItemKey::Genre, String::from(genre));
                true
            } else {
                false
            }
        } else {
            false
        }
    };

    if applied {
        use lofty::file::AudioFile;
        tagged.save_to_path(
            path,
            lofty::config::WriteOptions::default(),
        )?;
    }

    Ok(())
}
