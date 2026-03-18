use std::fs::File;
use std::io::BufReader;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use std::thread;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use cpal::traits::{DeviceTrait, HostTrait};

struct AudioState {
    player: Player,
    _sink: MixerDeviceSink,
}

static AUDIO: OnceLock<Mutex<Option<AudioState>>> = OnceLock::new();
static CURRENT_SONG: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn get_current_song() -> &'static Mutex<Option<String>> {
    CURRENT_SONG.get_or_init(|| Mutex::new(None))
}

fn get_state() -> &'static Mutex<Option<AudioState>> {
    AUDIO.get_or_init(|| Mutex::new(None))
}

pub fn init() {
    reinit_audio();
    watch_device_changes();
}

fn reinit_audio() {
    let mut sink = DeviceSinkBuilder::from_default_device()
        .expect("Failed to get default device")
        .with_error_callback(|err| {
            eprintln!("Audio stream error: {err}, attempting reinit...");
            reinit_audio();
        })
        .open_stream()
        .expect("Failed to open audio stream");

    sink.log_on_drop(false);

    let player = Player::connect_new(&sink.mixer());

    let mut state = get_state().lock().unwrap();
    *state = Some(AudioState {
        player,
        _sink: sink,
    });
}

fn watch_device_changes() { //lets not start a memory leak next time yeah?
    thread::spawn(|| {
        let mut last_device = default_device_name();
        loop {
            thread::sleep(Duration::from_secs(2));
            let current = default_device_name();
            if current != last_device {
                eprintln!("Default audio device changed: {:?} -> {:?}", last_device, current);
                last_device = current;
                reinit_audio();
            }
        }
    });
}

fn default_device_name() -> Option<String> {
    cpal::default_host()
        .default_output_device()
        .and_then(|d| d.name().ok())
}

pub fn load_song(path: &str) {
    eprintln!("load_song called with path: {}", path);
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => { eprintln!("Could not open '{}': {}", path, e); return; }
    };
    let source = match Decoder::new(BufReader::new(file)) {
        Ok(s) => s,
        Err(e) => { eprintln!("Could not decode audio: {}", e); return; }
    };
    // Save path before locking audio state to avoid any lock ordering issues
    {
        *get_current_song().lock().unwrap() = Some(path.to_string());
    }
    if let Some(state) = get_state().lock().unwrap().as_ref() {
        state.player.stop();
        state.player.append(source);
        state.player.pause();
        eprintln!("load_song: appended and paused");
    } else {
        eprintln!("load_song: audio state not initialized!");
    }
}

pub fn start_playback() {
    if let Some(state) = get_state().lock().unwrap().as_ref() {
        state.player.play();
    }
}

pub fn stop_playback() {
    if let Some(state) = get_state().lock().unwrap().as_ref() {
        state.player.pause();
    }
}

pub fn toggle_playback() {
    if let Some(state) = get_state().lock().unwrap().as_ref() {
        if state.player.is_paused() {
            state.player.play();
        } else {
            state.player.pause();
        }
    }
}

pub fn rewind_playback() {
    let was_playing = if let Some(state) = get_state().lock().unwrap().as_ref() {
        !state.player.is_paused()
    } else { false };

    let current = get_current_song().lock().unwrap().clone();

    if let Some(state) = get_state().lock().unwrap().as_ref() {
        state.player.stop();
    }

    if let Some(path) = current {
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(e) => { eprintln!("Could not open '{}': {}", path, e); return; }
        };
        let source = match Decoder::new(BufReader::new(file)) {
            Ok(s) => s,
            Err(e) => { eprintln!("Could not decode audio: {}", e); return; }
        };
        if let Some(state) = get_state().lock().unwrap().as_ref() {
            state.player.append(source);
        }
    }

    if was_playing {
        start_playback();
    }
}

pub fn fast_forward_playback() {
    if let Some(state) = get_state().lock().unwrap().as_ref() {
        let pos = state.player.get_pos();
        state.player.try_seek(pos + Duration::from_secs(10)).ok();
    }
}