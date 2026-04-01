use macroquad::prelude::*;
use once_cell::sync::OnceCell;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::collections::{HashSet, VecDeque};
use crate::library::{self, LibraryNode};
use crate::player;
use rfd::FileDialog;

static COLLAPSED_FOLDERS: OnceCell<Mutex<HashSet<String>>> = OnceCell::new();

static IS_PLAYING: AtomicBool = AtomicBool::new(false);
static SELECTED_SONG: OnceCell<Mutex<Option<String>>> = OnceCell::new();
static VISUALIZER_TYPE: OnceCell<Mutex<char>> = OnceCell::new();

//--other statics--//
static LOGO_TEXTURE: OnceCell<Texture2D> = OnceCell::new();
static FONT: OnceCell<Font> = OnceCell::new();
static WAVE_HISTORY: OnceCell<Mutex<VecDeque<Vec<f32>>>> = OnceCell::new();
static WAVE_SCROLL_OFFSET: OnceCell<Mutex<f32>> = OnceCell::new();

thread_local! {
    // (cached_song_path, texture)
    static ALBUM_ART_TEX: RefCell<Option<(String, Texture2D)>> = RefCell::new(None);
}

//--colors--//

const BG_COLOR: Color = Color::new(53.0/255.0, 16.0/255.0, 108.0/255.0, 1.0);
const SIDEBAR_BG_COLOR: Color = Color::new(100.0/255.0, 100.0/255.0, 100.0/255.0, 1.0);
const HIGHLIGHT_COLOR: Color = Color::new(1.0, 1.0, 1.0, 0.15);

//--main ui drawing function--//
pub fn draw_main_ui() {

    let w = screen_width().max(512.0);
    let h = screen_height().max(512.0);

    let (br, bg, bb) = player::get_bg_color();
    clear_background(Color::new(br, bg, bb, 1.0));

    draw_visualizer(w,h);


    // --sidebar/bottom bar--//
    // sidebar
    let sidebar_x = 30.0;
    let sidebar_y = 30.0;
    let sidebar_w = 240.0;
    let sidebar_h = h - 180.0;
    draw_rectangle(sidebar_x, sidebar_y, sidebar_w, sidebar_h, SIDEBAR_BG_COLOR);
    draw_library(sidebar_x, sidebar_y, sidebar_w, sidebar_h - 40.0);

    // bottom bar things
    draw_rectangle(30.0, h - 120.0, w - 60.0, 90.0, SIDEBAR_BG_COLOR);

    //player items.
    draw_play_pause_button(w,h);
    draw_rewind_button(w,h);
    draw_fast_forward_button(w,h);

    //visualizer stuffs
    draw_visualizer_type_buttons();

    draw_library_folder_button(sidebar_x, sidebar_y, sidebar_w, sidebar_h);

    draw_now_playing(h);
    //println!("Screen dimensions: {}x{}", w, h);
}

fn draw_visualizer(w: f32, h: f32) {
    let vis_x = (w - (w - 300.0)) as i32;
    let vis_y = 30_i32;
    let vis_w = (w - 330.0) as i32;
    let vis_h = (h - 180.0) as i32;

    if vis_w <= 0 || vis_h <= 0 { return; }

    let vis_type = *VISUALIZER_TYPE.get_or_init(|| Mutex::new('2')).lock().unwrap();
    if vis_type == '3' {
        draw_visualizer_3d(vis_x, vis_y, vis_w, vis_h);
    } else if vis_type == 'w' {
        draw_visualizer_sinewave_3d(vis_x, vis_y, vis_w, vis_h);
    } else {
        draw_visualizer_2d(vis_x, vis_y, vis_w, vis_h);
    }

    draw_rectangle_lines(
        vis_x as f32,
        vis_y as f32,
        vis_w as f32,
        vis_h as f32,
        4.0,
        WHITE,
    );
}

fn draw_visualizer_2d(vis_x: i32, vis_y: i32, vis_w: i32, vis_h: i32) {
    let num_bins: usize = 64;
    let spectrum = player::get_spectrum(num_bins);
    let bar_w = vis_w as f32 / num_bins as f32;
    for (i, &mag) in spectrum.iter().enumerate() {
        let bar_h = mag * (vis_h as f32 - 4.0);
        let bx = vis_x as f32 + i as f32 * bar_w;
        let by = vis_y as f32 + vis_h as f32 - bar_h - 2.0;
        let t = i as f32 / num_bins as f32;
        let bar_color = Color::new(t, 0.2, 1.0 - t, 0.9);
        draw_rectangle(bx + 1.0, by, bar_w - 2.0, bar_h, bar_color);
    }
}

fn draw_visualizer_3d(vis_x: i32, vis_y: i32, vis_w: i32, vis_h: i32) {
    let screen_h = screen_height() as i32;
    // OpenGL scissor origin is bottom-left; macroquad vis_y is top-left
    let gl_y = screen_h - vis_y - vis_h;

    unsafe {
        miniquad::gl::glEnable(miniquad::gl::GL_SCISSOR_TEST);
        miniquad::gl::glScissor(vis_x, gl_y, vis_w, vis_h);
    }

    // Camera3D viewport also uses OpenGL bottom-left convention
    set_camera(&Camera3D {
        position: vec3(0.0, 4.5, 9.0),
        target: vec3(0.0, 0.5, 0.0),
        up: vec3(0.0, 1.0, 0.0),
        viewport: Some((vis_x, gl_y, vis_w, vis_h)),
        ..Default::default()
    });

    let num_bins: usize = 48;
    let spectrum = player::get_spectrum(num_bins);

    // tan(22.5°) = √2 - 1 — half the horizontal FOV span per unit of z distance
    // (macroquad Camera3D uses 45° vFOV and scales horizontally by aspect)
    let aspect = vis_w as f32 / vis_h as f32;
    let cam_z = 9.0_f32;
    let visible_half_w = cam_z * (std::f32::consts::SQRT_2 - 1.0) * aspect;
    // fit all bars into 90% of the visible width; never exceed a comfortable max
    let spacing = ((visible_half_w * 2.0 * 0.95) / num_bins as f32).min(0.42);
    let total_w = num_bins as f32 * spacing;
    let bar_depth = (spacing * 0.80).min(0.35);

    for (i, &mag) in spectrum.iter().enumerate() {
        let x = -total_w / 2.0 + i as f32 * spacing + spacing / 2.0;
        let bar_h = (mag * 5.0).max(0.02);
        let t = i as f32 / num_bins as f32;
        let bar_color = Color::new(t, 0.15 + mag * 0.5, 1.0 - t, 1.0);
        draw_cube(
            vec3(x, bar_h / 2.0, 0.0),
            vec3(spacing * 0.80, bar_h, bar_depth),
            None,
            bar_color,
        );
    }

    // Subtle floor grid
    draw_grid(20, spacing, Color::new(1.0, 1.0, 1.0, 0.08), Color::new(1.0, 1.0, 1.0, 0.04));

    set_default_camera();

    unsafe {
        miniquad::gl::glDisable(miniquad::gl::GL_SCISSOR_TEST);
        // Explicitly restore the full-screen GL viewport.
        // macroquad's set_default_camera() restores the projection matrix but does
        // NOT call glViewport when a Camera3D viewport was active, so subsequent
        // 2D draws (sidebar, bottom bar) get clipped to the old 3D viewport region.
        miniquad::gl::glViewport(0, 0, screen_width() as i32, screen_height() as i32);
    }
}

fn draw_visualizer_sinewave_3d(vis_x: i32, vis_y: i32, vis_w: i32, vis_h: i32) {
    const NUM_POINTS: usize = 256;
    const MAX_HISTORY: usize = 36;
    // How many slots per second the waterfall scrolls (slots/sec = 1 / PUSH_INTERVAL_SECS)
    const PUSH_INTERVAL_SECS: f32 = 0.08; // 80ms between captures

    let dt = get_frame_time();

    // Advance continuous scroll offset every frame
    let scroll_frac = {
        let mut offset = WAVE_SCROLL_OFFSET
            .get_or_init(|| Mutex::new(0.0_f32))
            .lock().unwrap();
        *offset += dt / PUSH_INTERVAL_SECS;
        // When offset hits 1.0, capture a new frame and roll over
        if *offset >= 1.0 {
            *offset -= 1.0;
            let samples = player::get_samples(NUM_POINTS);
            let hist = WAVE_HISTORY.get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_HISTORY)));
            let mut h = hist.lock().unwrap();
            h.push_front(samples);
            if h.len() > MAX_HISTORY { h.pop_back(); }
        }
        *offset
    };

    let screen_h = screen_height() as i32;
    let gl_y = screen_h - vis_y - vis_h;

    unsafe {
        miniquad::gl::glEnable(miniquad::gl::GL_SCISSOR_TEST);
        miniquad::gl::glScissor(vis_x, gl_y, vis_w, vis_h);
    }

    set_camera(&Camera3D {
        position: vec3(0.0, 3.5, 7.5),
        target:   vec3(0.0, 0.0, -10.0),
        up:       vec3(0.0, 1.0, 0.0),
        viewport: Some((vis_x, gl_y, vis_w, vis_h)),
        ..Default::default()
    });

    let hist = WAVE_HISTORY.get_or_init(|| Mutex::new(VecDeque::new()));
    let h = hist.lock().unwrap();

    let wave_width = 12.0_f32;
    let z_spacing  = 0.57_f32;
    let amp_scale  = 2.6_f32;

    for (frame_idx, frame) in h.iter().enumerate() {
        // scroll_frac (0..1) smoothly shifts every slot backward continuously
        let z_slot = frame_idx as f32 + scroll_frac;
        let age   = z_slot / MAX_HISTORY as f32;
        let alpha = (1.0 - age * 0.88).max(0.05);
        let z     = -(z_slot * z_spacing);

        // Hue shifts from cyan-purple (new) to deep blue (old)
        let r = (0.4 - age * 0.4).max(0.0);
        let g = (0.6 - age * 0.5).max(0.0) * alpha;
        let b = 1.0 * alpha;
        let color = Color::new(r, g, b, alpha);

        for i in 1..frame.len() {
            let t0 = (i - 1) as f32 / (frame.len() - 1) as f32;
            let t1 = i as f32           / (frame.len() - 1) as f32;
            let x0 = -wave_width / 2.0 + t0 * wave_width;
            let x1 = -wave_width / 2.0 + t1 * wave_width;
            let y0 = frame[i - 1] * amp_scale;
            let y1 = frame[i]     * amp_scale;
            draw_line_3d(vec3(x0, y0, z), vec3(x1, y1, z), color);
        }
    }

    set_default_camera();

    unsafe {
        miniquad::gl::glDisable(miniquad::gl::GL_SCISSOR_TEST);
        miniquad::gl::glViewport(0, 0, screen_width() as i32, screen_height() as i32);
    }
}

fn draw_now_playing(h: f32) {
    let bar_y    = h - 120.0;
    let art_size = 70.0;
    let art_x    = 40.0;
    let art_y    = bar_y + 10.0;

    // Refresh texture cache when song changes
    let current_path = player::get_current_song_path();
    ALBUM_ART_TEX.with(|cell| {
        let mut cache = cell.borrow_mut();
        let stale = match &*cache {
            Some((cached, _)) => Some(cached.clone()) != current_path,
            None              => current_path.is_some(),
        };
        if stale {
            *cache = current_path.as_ref().and_then(|p| {
                player::get_current_album_art_rgba().map(|(w, h, rgba)| {
                    let tex = Texture2D::from_rgba8(w as u16, h as u16, &rgba);
                    (p.clone(), tex)
                })
            });
        }

        // Draw album art or placeholder
        match &*cache {
            Some((_, tex)) => {
                draw_texture_ex(tex, art_x, art_y, WHITE, DrawTextureParams {
                    dest_size: Some(vec2(art_size, art_size)),
                    ..Default::default()
                });
            }
            None => {
                draw_rectangle(art_x, art_y, art_size, art_size,
                    Color::new(0.25, 0.25, 0.35, 1.0));
                draw_text_ex("♪", art_x + 18.0, art_y + 48.0, TextParams {
                    font: FONT.get(), font_size: 36, color: WHITE, ..Default::default()
                });
            }
        }
    });

    // Draw title / artist / album text to the right of the art
    if let Some((title, artist, album)) = player::get_current_song_info() {
        let tx = art_x + art_size + 10.0;
        let max_w = 220.0;
        draw_text_ex(&truncate_to_fit(&title, max_w, 17), tx, art_y + 19.0, TextParams {
            font: FONT.get(), font_size: 17, color: WHITE, ..Default::default()
        });
        if !artist.is_empty() {
            draw_text_ex(&truncate_to_fit(&artist, max_w, 14), tx, art_y + 38.0, TextParams {
                font: FONT.get(), font_size: 14, color: LIGHTGRAY, ..Default::default()
            });
        }
        if !album.is_empty() {
            draw_text_ex(&truncate_to_fit(&album, max_w, 14), tx, art_y + 56.0, TextParams {
                font: FONT.get(), font_size: 14, color: LIGHTGRAY, ..Default::default()
            });
        }
    }
}

fn draw_library_folder_button(sidebar_x: f32, sidebar_y: f32, sidebar_w: f32, sidebar_h: f32) {
    let btn_h = 30.0;
    let btn_y = sidebar_y + sidebar_h - btn_h;
    if draw_button_rect(sidebar_x, btn_y, sidebar_w, btn_h, "Folder", LIGHTGRAY, SIDEBAR_BG_COLOR, BLACK, WHITE, WHITE) {
        if let Some(path) = FileDialog::new().pick_folder() {
            library::set_root(&path.to_string_lossy());
        }
    }
}

fn draw_library(x: f32, y: f32, _w: f32, h: f32) {
    let mut current_y = y + 10.0;
    library::with_library(|root| {
        if let Some(node) = root {
            draw_node(node, 0, x, y + h, y, &mut current_y, true, &vec![], "root");
        }
    });
}

fn draw_node(node: &LibraryNode, depth: u32, x: f32, max_y: f32, min_y: f32, y: &mut f32, is_last: bool, open_depths: &Vec<bool>, path: &str) {
    if *y > max_y { return; }

    let font_size = 16;
    let line_height = 20.0;

    let mut prefix = String::new();
    for i in 0..depth as usize {
        if i == depth as usize - 1 {
            if is_last { prefix.push_str("└─ "); } else { prefix.push_str("├─ "); }
        } else if open_depths[i] {
            prefix.push_str("│  ");
        } else {
            prefix.push_str("   ");
        }
    }

    let is_folder = matches!(node, LibraryNode::Folder { .. });
    let collapsed = is_folder && get_collapsed().contains(path);

    let (label, color) = match node {
        LibraryNode::Folder { name, .. } => {
            let arrow = if collapsed { "▶ " } else { "▼ " };
            (format!("{}{}{}/", prefix, arrow, name), WHITE)
        },
        LibraryNode::Track(song) => (format!("{}{}", prefix, song.title), LIGHTGRAY),
    };
    let label = truncate_to_fit(&label, 220.0, font_size);

    let text_y = *y + font_size as f32;

    // Check click on this row
    let mouse = mouse_position();
    let row_x = x + 5.0;
    let row_w = 220.0;
    let row_h = line_height;
    let row_clicked = mouse.0 >= row_x && mouse.0 <= row_x + row_w
        && mouse.1 >= *y && mouse.1 <= *y + row_h
        && is_mouse_button_pressed(MouseButton::Left);

    if is_folder {
        if row_clicked {
            let mut collapsed_set = get_collapsed();
            if collapsed_set.contains(path) {
                collapsed_set.remove(path);
            } else {
                collapsed_set.insert(path.to_string());
            }
        }
    } else if let LibraryNode::Track(song) = node {
        if row_clicked {
            IS_PLAYING.store(false, Ordering::Relaxed);

            // Build a queue of sibling audio files (same folder, no subdirs)
            let audio_exts = ["mp3", "wav", "flac", "ogg", "aac", "m4a"];
            let mut siblings: Vec<String> = std::path::Path::new(&song.path)
                .parent()
                .and_then(|dir| std::fs::read_dir(dir).ok())
                .map(|entries| {
                    let mut v: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().is_file())
                        .filter(|e| {
                            e.path().extension()
                                .and_then(|x| x.to_str())
                                .map(|x| audio_exts.contains(&x))
                                .unwrap_or(false)
                        })
                        .map(|e| e.path().to_string_lossy().to_string())
                        .collect();
                    v.sort();
                    v
                })
                .unwrap_or_default();
            if siblings.is_empty() { siblings.push(song.path.clone()); }
            let idx = siblings.iter().position(|p| p == &song.path).unwrap_or(0);

            player::load_song(&song.path);
            player::set_queue(siblings, idx);
            *SELECTED_SONG.get_or_init(|| Mutex::new(None)).lock().unwrap() =
                Some(song.path.clone());
        }
    }

    // Draw highlight behind selected track
    if let LibraryNode::Track(song) = node {
        let selected = SELECTED_SONG
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap();
        if selected.as_deref() == Some(song.path.as_str()) {
            draw_rectangle(row_x, *y, row_w, row_h, HIGHLIGHT_COLOR);
        }
    }

    draw_text_ex(&label, x + 5.0, text_y, TextParams {
        font: FONT.get(),
        font_size,
        color,
        ..Default::default()
    });
    *y += line_height;

    // Skip children if collapsed
    if collapsed { return; }

    if let LibraryNode::Folder { name, children } = node {
        let len = children.len();
        let mut new_depths = open_depths.clone();
        new_depths.push(!is_last);
        for (i, child) in children.iter().enumerate() {
            let child_path = format!("{}/{}", path, name);
            draw_node(child, depth + 1, x, max_y, min_y, y, i == len - 1, &new_depths, &child_path);
        }
    }
}

fn get_collapsed() -> std::sync::MutexGuard<'static, HashSet<String>> {
    COLLAPSED_FOLDERS.get_or_init(|| Mutex::new(HashSet::new())).lock().unwrap()
}
fn truncate_to_fit(text: &str, max_width: f32, font_size: u16) -> String {
    let font = FONT.get();
    let mut result = text.to_string();
    while !result.is_empty() {
        let dims = measure_text(&result, font, font_size, 1.0);
        if dims.width <= max_width {
            break;
        }
        result.pop();
    }
    if result.len() < text.len() {
        result.push('…');
    }
    result
}

fn draw_button_rect(x: f32, y: f32, w: f32, h: f32, label: &str, ch: Color, c: Color, tch: Color, tc: Color, outline: Color) -> bool {
    let mouse = mouse_position();
    let hovered = mouse.0 > x && mouse.0 < x + w && mouse.1 > y && mouse.1 < y + h;
    let clicked = hovered && is_mouse_button_pressed(MouseButton::Left);

    let color = if hovered { ch } else { c };
    let text_color = if hovered { tch } else { tc };
    draw_rectangle(x, y, w, h, color);
    draw_rectangle_lines(x, y, w, h, 6.0, outline);
    draw_text_ex(label, x + 10.0, y + h / 2.0 + 8.0, TextParams {
        font: FONT.get(),
        font_size: 24,
        color: text_color,
        ..Default::default()
    });

    clicked
}

fn draw_visualizer_type_buttons() {
    let current = *VISUALIZER_TYPE.get_or_init(|| Mutex::new('2')).lock().unwrap();

    // 2D button
    let is_2d_selected = current == '2';
    let (bg_2d, text_2d) = if is_2d_selected {
        (LIGHTGRAY, BLACK)
    } else {
        (SIDEBAR_BG_COLOR, WHITE)
    };
    if draw_button_rect(
        300.0, 30.0, 50.0, 35.0,
        "2D", LIGHTGRAY, bg_2d, BLACK, text_2d, WHITE,
    ) {
        *VISUALIZER_TYPE.get_or_init(|| Mutex::new('2')).lock().unwrap() = '2';
    }

    // 3D button
    let is_3d_selected = current == '3';
    let (bg_3d, text_3d) = if is_3d_selected {
        (LIGHTGRAY, BLACK)
    } else {
        (SIDEBAR_BG_COLOR, WHITE)
    };
    if draw_button_rect(
        350.0, 30.0, 50.0, 35.0,
        "3D", LIGHTGRAY, bg_3d, BLACK, text_3d, WHITE,
    ) {
        *VISUALIZER_TYPE.get_or_init(|| Mutex::new('2')).lock().unwrap() = '3';
    }

    // Wave button
    let is_wave_selected = current == 'w';
    let (bg_w, text_w) = if is_wave_selected {
        (LIGHTGRAY, BLACK)
    } else {
        (SIDEBAR_BG_COLOR, WHITE)
    };
    if draw_button_rect(
        405.0, 30.0, 80.0, 35.0,
        "Wave", LIGHTGRAY, bg_w, BLACK, text_w, WHITE,
    ) {
        *VISUALIZER_TYPE.get_or_init(|| Mutex::new('2')).lock().unwrap() = 'w';
    }
}

fn draw_fast_forward_button(w: f32, h: f32) -> bool {
    if draw_button_rect((w/2.0)+20.0, h - 90.0, 40.0, 40.0, ">>", LIGHTGRAY, SIDEBAR_BG_COLOR, BLACK, BLACK, BLACK) {
        IS_PLAYING.store(true, Ordering::Relaxed);
        player::next_in_queue();
        // keep SELECTED_SONG in sync
        if let Some(p) = player::get_current_song_path() {
            *SELECTED_SONG.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(p);
        }
    }
    false
}

fn draw_rewind_button(w: f32, h: f32) -> bool {
    if draw_button_rect((w/2.0)-60.0, h - 90.0, 40.0, 40.0, "<<", LIGHTGRAY, SIDEBAR_BG_COLOR, BLACK, BLACK, BLACK) {
        IS_PLAYING.store(true, Ordering::Relaxed);
        player::prev_in_queue();
        if let Some(p) = player::get_current_song_path() {
            *SELECTED_SONG.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(p);
        }
    }
    false
}

fn draw_play_pause_button(w: f32, h: f32) -> bool {
    let label = if IS_PLAYING.load(Ordering::Relaxed) { "⏸︎" } else { "▶" };
    if draw_button_rect((w/2.0)-20.0, h - 90.0, 40.0, 40.0, label, LIGHTGRAY, SIDEBAR_BG_COLOR, BLACK, BLACK, BLACK) {
        IS_PLAYING.store(!IS_PLAYING.load(Ordering::Relaxed), Ordering::Relaxed);
        if IS_PLAYING.load(Ordering::Relaxed) {
            player::start_playback();
        } else {
            player::stop_playback();
        }
    }

    IS_PLAYING.load(Ordering::Relaxed)
}

pub async fn load_assets() {
    // Font is embedded at compile time — works in any deployed binary with no asset folder needed.
    match load_ttf_font_from_bytes(include_bytes!("../assets/fonts/DejaVuSansMono.ttf")) {
        Ok(font) => { FONT.set(font).ok(); }
        Err(e) => { eprintln!("Failed to load embedded font: {}", e); }
    }
    // Texture is optional — only used as a logo, failure is non-fatal.
    if let Ok(tex) = load_texture("assets/images/Internet_Example.png").await {
        LOGO_TEXTURE.set(tex).ok();
    }
}
