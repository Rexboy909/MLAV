use id3::{Tag, TagLike};
use std::path::Path;

pub struct Song {
    pub title: String,
    pub artist: String,
    pub duration: u32,
    pub path: String,
}

impl Song {
    pub fn from_path(path: &Path) -> Option<Song> {
        let tag = Tag::read_from_path(path).ok()?;
        Some(Song {
            title: tag.title().unwrap_or("Unknown Title").to_string(),
            artist: tag.artist().unwrap_or("Unknown Artist").to_string(),
            duration: tag.duration().unwrap_or(0),
            path: path.to_string_lossy().to_string(),
        })
    }
}