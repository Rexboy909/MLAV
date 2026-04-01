use lofty::prelude::*;
use lofty::probe::Probe;
use std::path::Path;

pub struct Song {
    pub title: String,
    pub artist: String,
    pub duration: u32,
    pub path: String,
}

impl Song {
    pub fn from_path(path: &Path) -> Option<Song> {
        let tagged_file = Probe::open(path).ok()?.read().ok()?;
        let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag())?;
        Some(Song {
            title: tag.title().as_deref().unwrap_or("Unknown Title").to_string(),
            artist: tag.artist().as_deref().unwrap_or("Unknown Artist").to_string(),
            duration: 0,
            path: path.to_string_lossy().to_string(),
        })
    }
}