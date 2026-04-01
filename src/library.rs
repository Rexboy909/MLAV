use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use crate::song::Song;

// -- tree structure --//
pub enum LibraryNode {
    Folder {
        name: String,
        children: Vec<LibraryNode>,
    },
    Track(Song),
}

// -- global state --//
static LIBRARY: OnceLock<Mutex<Option<LibraryNode>>> = OnceLock::new();
static LIBRARY_ROOT: OnceLock<Mutex<String>> = OnceLock::new();

fn get_library() -> &'static Mutex<Option<LibraryNode>> {
    LIBRARY.get_or_init(|| Mutex::new(None))
}

fn get_root() -> &'static Mutex<String> {
    LIBRARY_ROOT.get_or_init(|| Mutex::new(String::new()))
}

pub fn init(dir: &str) {
    *get_root().lock().unwrap() = dir.to_string();
    *get_library().lock().unwrap() = Some(build_tree(dir));
}

/// Change the library root at runtime (picked from the file dialog).
pub fn set_root(dir: &str) {
    init(dir);
}

pub fn watch() {
    thread::spawn(|| {
        loop {
            thread::sleep(Duration::from_secs(5));
            let dir = get_root().lock().unwrap().clone();
            if !dir.is_empty() {
                *get_library().lock().unwrap() = Some(build_tree(&dir));
            }
        }
    });
}

pub fn with_library<F>(f: F) where F: FnOnce(Option<&LibraryNode>) {
    let lock = get_library().lock().unwrap();
    f(lock.as_ref());
}

// -- tree building --//
fn build_tree(dir: &str) -> LibraryNode {
    build_node(Path::new(dir))
}

fn is_audio_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("mp3") | Some("wav") | Some("flac") | Some("ogg")
    )
}

fn build_node(path: &Path) -> LibraryNode {
    if path.is_dir() {
        let name = path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let mut children: Vec<LibraryNode> = std::fs::read_dir(path)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| build_node(&e.path()))
            .filter(|n| match n {
                LibraryNode::Folder { name, .. } => !name.is_empty(),
                LibraryNode::Track(_) => true,
            })
            .collect();

        children.sort_by(|a, b| {
            let a_name = match a { LibraryNode::Folder { name, .. } => name.as_str(), LibraryNode::Track(s) => s.title.as_str() };
            let b_name = match b { LibraryNode::Folder { name, .. } => name.as_str(), LibraryNode::Track(s) => s.title.as_str() };
            a_name.cmp(b_name)
        });

        LibraryNode::Folder { name, children }
    } else if is_audio_file(path) {
        LibraryNode::Track(Song::from_path(path).unwrap_or(Song {
            title: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            artist: "Unknown".to_string(),
            duration: 0,
            path: path.to_string_lossy().to_string(),
        }))
    } else {
        LibraryNode::Folder { name: String::new(), children: vec![] }
    }
}