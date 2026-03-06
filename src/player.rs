use std::fs::File;
use std::io::BufReader;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};

struct AudioState {
    player: Player,
    _sink: MixerDeviceSink,
}

static AUDIO: OnceLock<Mutex<AudioState>> = OnceLock::new();

pub fn init() {
    let sink = DeviceSinkBuilder::open_default_sink()
        .expect("Failed to open default audio stream");

    let player = Player::connect_new(&sink.mixer());

    AUDIO.set(Mutex::new(AudioState {
        player,
        _sink: sink,
    })).ok();
}

pub fn load_output() {
    let file = BufReader::new(File::open("songs/example.mp3").unwrap());
    let source = Decoder::new(file).unwrap();

    let audio = AUDIO.get().unwrap().lock().unwrap();
    audio.player.append(source);
    audio.player.pause();
}

pub fn start_playback() {
    AUDIO.get().unwrap().lock().unwrap().player.play();
}

pub fn stop_playback() {
    AUDIO.get().unwrap().lock().unwrap().player.pause();
}

pub fn toggle_playback() {
    let audio = AUDIO.get().unwrap().lock().unwrap();
    if audio.player.is_paused() {
        audio.player.play();
    } else {
        audio.player.pause();
    }
}

pub fn rewind_playback() {
    let audio = AUDIO.get().unwrap().lock().unwrap();
    audio.player.try_seek(Duration::ZERO).ok();
}

pub fn fast_forward_playback() {
    let audio = AUDIO.get().unwrap().lock().unwrap();
    let current = audio.player.get_pos();
    audio.player.try_seek(current + Duration::from_secs(10)).ok();
}