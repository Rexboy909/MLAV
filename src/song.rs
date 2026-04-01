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
        let stem = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown Title")
            .to_string();

        let tag = Tag::read_from_path(path).ok();
        Some(Song {
            title: tag.as_ref()
                .and_then(|t| t.title())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or(stem),
            artist: tag.as_ref()
                .and_then(|t| t.artist())
                .unwrap_or("Unknown Artist")
                .to_string(),
            duration: tag.as_ref()
                .and_then(|t| t.duration())
                .unwrap_or(0),
            path: path.to_string_lossy().to_string(),
        })
    }
}