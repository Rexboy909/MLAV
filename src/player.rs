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

//sample capture ring-buffer

const CAPTURE_LEN: usize = 4096; // keep the most recent N f32 samples

static SAMPLE_BUFFER: OnceLock<Arc<Mutex<VecDeque<f32>>>> = OnceLock::new();
static SAMPLE_RATE: OnceLock<Mutex<u32>> = OnceLock::new();
static SMOOTHED_SPECTRUM: OnceLock<Mutex<Vec<f32>>> = OnceLock::new();
static LAST_SPECTRUM_TIME: OnceLock<Mutex<std::time::Instant>> = OnceLock::new();

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
