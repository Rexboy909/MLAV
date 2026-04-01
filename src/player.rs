use std::fs::File;
use std::io::BufReader;
use std::num::NonZero;
use std::sync::{Arc, Mutex, OnceLock};
use std::collections::VecDeque;
use std::time::Duration;
use std::thread;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use rustfft::{FftPlanner, num_complex::Complex};
use cpal::traits::{DeviceTrait, HostTrait};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::picture::PictureType;

//sample capture ring-buffer

const CAPTURE_LEN: usize = 4096; // keep the most recent N f32 samples

static SAMPLE_BUFFER: OnceLock<Arc<Mutex<VecDeque<f32>>>> = OnceLock::new();
static SAMPLE_RATE: OnceLock<Mutex<u32>> = OnceLock::new();
static SMOOTHED_SPECTRUM: OnceLock<Mutex<Vec<f32>>> = OnceLock::new();
static LAST_SPECTRUM_TIME: OnceLock<Mutex<std::time::Instant>> = OnceLock::new();

// (title, artist, album)
static CURRENT_SONG_INFO: OnceLock<Mutex<Option<(String, String, String)>>> = OnceLock::new();
// (width, height, rgba_bytes)
static CURRENT_ALBUM_ART: OnceLock<Mutex<Option<(u32, u32, Vec<u8>)>>> = OnceLock::new();
// background color derived from album art average, darkened
static CURRENT_BG_COLOR: OnceLock<Mutex<(f32, f32, f32)>> = OnceLock::new();
// playback queue (sibling songs in the same folder)
static SONG_QUEUE: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static QUEUE_INDEX: OnceLock<Mutex<usize>> = OnceLock::new();

fn get_queue() -> &'static Mutex<Vec<String>> {
    SONG_QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}
fn get_queue_index() -> &'static Mutex<usize> {
    QUEUE_INDEX.get_or_init(|| Mutex::new(0))
}

fn get_bg_color_static() -> &'static Mutex<(f32, f32, f32)> {
    CURRENT_BG_COLOR.get_or_init(|| Mutex::new((0.18, 0.18, 0.22)))
}

pub fn get_current_song_path() -> Option<String> {
    get_current_song().lock().unwrap().clone()
}

pub fn get_current_song_info() -> Option<(String, String, String)> {
    CURRENT_SONG_INFO.get_or_init(|| Mutex::new(None)).lock().unwrap().clone()
}

pub fn get_current_album_art_rgba() -> Option<(u32, u32, Vec<u8>)> {
    CURRENT_ALBUM_ART.get_or_init(|| Mutex::new(None)).lock().unwrap().clone()
}

pub fn get_bg_color() -> (f32, f32, f32) {
    *get_bg_color_static().lock().unwrap()
}

/// Set the playback queue and immediately play the entry at `current_idx`.
pub fn set_queue(paths: Vec<String>, current_idx: usize) {
    *get_queue_index().lock().unwrap() = current_idx;
    *get_queue().lock().unwrap() = paths;
}

pub fn next_in_queue() {
    let path = {
        let q = get_queue().lock().unwrap();
        if q.is_empty() { return; }
        let mut idx = get_queue_index().lock().unwrap();
        *idx = (*idx + 1) % q.len();
        q[*idx].clone()
    };
    load_song(&path);
    start_playback();
}

pub fn prev_in_queue() {
    let path = {
        let q = get_queue().lock().unwrap();
        if q.is_empty() { return; }
        let mut idx = get_queue_index().lock().unwrap();
        *idx = idx.checked_sub(1).unwrap_or(q.len() - 1);
        q[*idx].clone()
    };
    load_song(&path);
    start_playback();
}

fn get_last_spectrum_time() -> &'static Mutex<std::time::Instant> {
    LAST_SPECTRUM_TIME.get_or_init(|| Mutex::new(std::time::Instant::now()))
}

fn sample_buffer() -> Arc<Mutex<VecDeque<f32>>> {
    SAMPLE_BUFFER.get_or_init(|| Arc::new(Mutex::new(VecDeque::with_capacity(CAPTURE_LEN)))).clone()
}

/// A thin `Source` wrapper that copies every sample it yields into a shared ring buffer.
struct SampleCapture<S: Source<Item = f32> + Iterator<Item = f32>> {
    inner: S,
    buffer: Arc<Mutex<VecDeque<f32>>>,
    channels: u16,
    channel_acc: Vec<f32>, // accumulate one frame of channels before storing
}

impl<S: Source<Item = f32> + Iterator<Item = f32>> Iterator for SampleCapture<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let s = self.inner.next()?;
        self.channel_acc.push(s);
        // Once we have a full interleaved frame, downmix to mono and store
        if self.channel_acc.len() >= self.channels as usize {
            let mono = self.channel_acc.iter().sum::<f32>() / self.channels as f32;
            self.channel_acc.clear();
            if let Ok(mut buf) = self.buffer.try_lock() {
                buf.push_back(mono);
                if buf.len() > CAPTURE_LEN {
                    buf.pop_front();
                }
            }
        }
        Some(s)
    }
}

impl<S: Source<Item = f32> + Iterator<Item = f32>> Source for SampleCapture<S> {
    fn current_span_len(&self) -> Option<usize> { self.inner.current_span_len() }
    fn channels(&self) -> NonZero<u16> { self.inner.channels() }
    fn sample_rate(&self) -> NonZero<u32> { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<Duration> { self.inner.total_duration() }
}

//FFT helper

/// Returns `num_bins` frequency-magnitude values (linear scale, 0-based from DC).
/// Call this every frame from the UI to feed the visualizer.
pub fn get_spectrum(num_bins: usize) -> Vec<f32> {
    let raw: Vec<f32> = {
        let buf = sample_buffer();
        let guard = buf.lock().unwrap();
        guard.iter().copied().collect()
    };

    let fft_size = raw.len().next_power_of_two().min(CAPTURE_LEN);
    if fft_size < 2 { return vec![0.0; num_bins]; }

    // Hann window
    let mut input: Vec<Complex<f32>> = (0..fft_size)
        .map(|i| {
            let sample = *raw.get(raw.len().saturating_sub(fft_size) + i).unwrap_or(&0.0);
            let window = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (fft_size - 1) as f32).cos());
            Complex { re: sample * window, im: 0.0 }
        })
        .collect();

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(fft_size);
    fft.process(&mut input);

    let half = fft_size / 2;
    let scale = 1.0 / fft_size as f32;
    let mags: Vec<f32> = input[..half].iter().map(|c| c.norm() * scale).collect();

    // logarithmic frequency bucketing
    // Iterate over output bars
    let sample_rate = *SAMPLE_RATE.get_or_init(|| Mutex::new(44100)).lock().unwrap() as f32;
    let freq_per_bin = sample_rate / fft_size as f32;
    let f_min: f32 = 20.0;
    let f_max: f32 = (sample_rate / 2.0).min(20000.0);
    let log_min = f_min.log2();
    let log_max = f_max.log2();

    let mut out = vec![0.0f32; num_bins];
    for bar in 0..num_bins {
        // frequency range this bar covers
        let t_lo = bar as f32 / num_bins as f32;
        let t_hi = (bar + 1) as f32 / num_bins as f32;
        let freq_lo = 2.0_f32.powf(log_min + t_lo * (log_max - log_min));
        let freq_hi = 2.0_f32.powf(log_min + t_hi * (log_max - log_min));

        let i_lo = ((freq_lo / freq_per_bin) as usize).max(1).min(half - 1);
        let i_hi = ((freq_hi / freq_per_bin) as usize).max(1).min(half - 1);

        // if the range covers multiple FFT bins take the max; otherwise interpolate
        if i_hi > i_lo {
            out[bar] = mags[i_lo..=i_hi].iter().cloned().fold(0.0_f32, f32::max);
        } else {
            // sub-bin resolution: linearly interpolate between adjacent bins
            let frac = (freq_lo / freq_per_bin) - i_lo as f32;
            let a = mags[i_lo];
            let b = *mags.get(i_lo + 1).unwrap_or(&a);
            out[bar] = a + frac * (b - a);
        }
    }

    // dB magnitude scale → 0..1
    // maps [-80 dB, 0 dB] linearly onto [0.0, 1.0]
    const DB_FLOOR: f32 = -80.0;
    for v in out.iter_mut() {
        let db = 20.0 * v.log10().max(DB_FLOOR / 20.0);
        *v = ((db - DB_FLOOR) / (-DB_FLOOR)).clamp(0.0, 1.0);
    }

    // Time-based smoothing — frame-rate independent
    let dt = {
        let mut last = get_last_spectrum_time().lock().unwrap();
        let elapsed = last.elapsed().as_secs_f32().min(0.1); // cap so paused frames don't explode
        *last = std::time::Instant::now();
        elapsed
    };

    // Tune these: half-life = time for a value to travel halfway toward target
    const ATTACK_HALF_LIFE: f32 = 0.025; // 25ms — fast rise
    const DECAY_HALF_LIFE: f32  = 0.200; // 200ms — slower fall

    let attack_k = 1.0 - 0.5_f32.powf(dt / ATTACK_HALF_LIFE);
    let decay_k  = 0.5_f32.powf(dt / DECAY_HALF_LIFE);

    let smoothed = SMOOTHED_SPECTRUM.get_or_init(|| Mutex::new(vec![0.0; num_bins]));
    let mut sm = smoothed.lock().unwrap();
    if sm.len() != num_bins { *sm = vec![0.0; num_bins]; }
    for (s, &n) in sm.iter_mut().zip(out.iter()) {
        if n >= *s {
            *s += (n - *s) * attack_k; // lerp toward new peak
        } else {
            *s *= decay_k;             // exponential fall-off
        }
    }
    sm.clone()
}


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
    let decoder = match Decoder::new(BufReader::new(file)) {
        Ok(s) => s,
        Err(e) => { eprintln!("Could not decode audio: {}", e); return; }
    };

    // Store sample rate so get_spectrum can compute real Hz values
    let num_channels = decoder.channels().get();
    *SAMPLE_RATE.get_or_init(|| Mutex::new(44100)).lock().unwrap()
        = decoder.sample_rate().get();

    // Clear old samples so the visualizer doesn't show stale data
    sample_buffer().lock().unwrap().clear();

    let source = SampleCapture {
        inner: decoder,
        buffer: sample_buffer(),
        channels: num_channels,
        channel_acc: Vec::with_capacity(num_channels as usize),
    };

    {
        *get_current_song().lock().unwrap() = Some(path.to_string());

        // Read metadata using lofty — handles MP3/ID3, FLAC, OGG Vorbis, M4A/AAC, WAV
        let info_lock = CURRENT_SONG_INFO.get_or_init(|| Mutex::new(None));
        let art_lock  = CURRENT_ALBUM_ART.get_or_init(|| Mutex::new(None));

        let fallback_title = || std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        match Probe::open(path).and_then(|p| p.read()) {
            Ok(tagged_file) => {
                let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());
                if let Some(tag) = tag {
                    let title  = tag.title().as_deref().unwrap_or("Unknown Title").to_string();
                    let artist = tag.artist().as_deref().unwrap_or("").to_string();
                    let album  = tag.album().as_deref().unwrap_or("").to_string();
                    *info_lock.lock().unwrap() = Some((title, artist, album));

                    // Pick CoverFront, fall back to first available picture
                    let mut chosen: Option<&lofty::picture::Picture> = None;
                    for pic in tag.pictures() {
                        if chosen.is_none() { chosen = Some(pic); }
                        if pic.pic_type() == PictureType::CoverFront { chosen = Some(pic); break; }
                    }
                    let art = chosen.and_then(|p| {
                        image::load_from_memory(p.data()).ok().map(|img| {
                            let rgba = img.into_rgba8();
                            let (w, h) = rgba.dimensions();
                            let pixels = rgba.as_raw();
                            let (mut r, mut g, mut b, mut count) = (0u64, 0u64, 0u64, 0u64);
                            for chunk in pixels.chunks(4 * 8) {
                                if chunk.len() >= 4 {
                                    r += chunk[0] as u64;
                                    g += chunk[1] as u64;
                                    b += chunk[2] as u64;
                                    count += 1;
                                }
                            }
                            if count > 0 {
                                const DARKEN: f32 = 0.4;
                                *get_bg_color_static().lock().unwrap() = (
                                    (r as f32 / count as f32 / 255.0) * DARKEN,
                                    (g as f32 / count as f32 / 255.0) * DARKEN,
                                    (b as f32 / count as f32 / 255.0) * DARKEN,
                                );
                            }
                            (w, h, rgba.into_raw())
                        })
                    });
                    *art_lock.lock().unwrap() = art;
                    if chosen.is_none() {
                        *get_bg_color_static().lock().unwrap() = (0.18, 0.18, 0.22);
                    }
                } else {
                    *info_lock.lock().unwrap() = Some((fallback_title(), String::new(), String::new()));
                    *art_lock.lock().unwrap()  = None;
                    *get_bg_color_static().lock().unwrap() = (0.18, 0.18, 0.22);
                }
            }
            Err(_) => {
                *info_lock.lock().unwrap() = Some((fallback_title(), String::new(), String::new()));
                *art_lock.lock().unwrap()  = None;
                *get_bg_color_static().lock().unwrap() = (0.18, 0.18, 0.22);
            }
        }
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

pub fn get_pos() -> Duration {
    get_state().lock().unwrap().as_ref()
        .map(|s| s.player.get_pos())
        .unwrap_or(Duration::ZERO)
}

pub fn rewind_playback() {
    // If more than 3 s in, restart the current song; otherwise go to previous.
    if get_pos() > Duration::from_secs(3) {
        restart_current();
    } else {
        prev_in_queue();
    }
}

fn restart_current() {
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
        let decoder = match Decoder::new(BufReader::new(file)) {
            Ok(s) => s,
            Err(e) => { eprintln!("Could not decode audio: {}", e); return; }
        };
        sample_buffer().lock().unwrap().clear();
        let num_channels = decoder.channels().get();
        let source = SampleCapture {
            inner: decoder,
            buffer: sample_buffer(),
            channels: num_channels,
            channel_acc: Vec::with_capacity(num_channels as usize),
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

/// Returns the most recent `num_samples` raw mono waveform samples (−1..1).
/// Used by the sinewave visualizer.
pub fn get_samples(num_samples: usize) -> Vec<f32> {
    let buf = sample_buffer();
    let guard = buf.lock().unwrap();
    if guard.is_empty() { return vec![0.0; num_samples]; }
    let len = guard.len();
    let start = len.saturating_sub(num_samples);
    (0..num_samples)
        .map(|i| *guard.get(start + i.min(len - start - 1)).unwrap_or(&0.0))
        .collect()
}
