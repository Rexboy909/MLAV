use macroquad::prelude::*;
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::collections::HashSet;
use crate::library::{self, LibraryNode};
use crate::player;

static COLLAPSED_FOLDERS: OnceCell<Mutex<HashSet<String>>> = OnceCell::new();

static IS_PLAYING: AtomicBool = AtomicBool::new(false);
static SELECTED_SONG: OnceCell<Mutex<Option<String>>> = OnceCell::new();

//--other statics--//
static LOGO_TEXTURE: OnceCell<Texture2D> = OnceCell::new();
static FONT: OnceCell<Font> = OnceCell::new();

//--colors--//

const BG_COLOR: Color = Color::new(53.0/255.0, 16.0/255.0, 108.0/255.0, 1.0);
const SIDEBAR_BG_COLOR: Color = Color::new(100.0/255.0, 100.0/255.0, 100.0/255.0, 1.0);
const HIGHLIGHT_COLOR: Color = Color::new(1.0, 1.0, 1.0, 0.15);

//--main ui drawing function--//
pub fn draw_main_ui() {

    let w = screen_width().max(512.0);
    let h = screen_height().max(512.0);

    clear_background(BG_COLOR);

    draw_visualizer(w,h);


    // --sidebar/bottom bar--//
    // sidebar
    let sidebar_x = 30.0;
    let sidebar_y = 30.0;
    let sidebar_w = 180.0;
    let sidebar_h = h - 180.0;
    draw_rectangle(sidebar_x, sidebar_y, sidebar_w, sidebar_h, SIDEBAR_BG_COLOR);
    draw_library(sidebar_x, sidebar_y, sidebar_w, sidebar_h);

    // bottom bar things
    draw_rectangle(30.0, h - 120.0, w - 60.0, 90.0, SIDEBAR_BG_COLOR);

    //player items.
    draw_play_pause_button(w,h);
    draw_rewind_button(w,h);
    draw_fast_forward_button(w,h);
    //println!("Screen dimensions: {}x{}", w, h);
}

fn draw_visualizer(w: f32, h: f32) {
    let vis_x = (w - (w - 250.0)) as i32;
    let vis_y = 30_i32;
    let vis_w = (w - 280.0) as i32;
    let vis_h = (h - 180.0) as i32;

    unsafe {
        miniquad::gl::glEnable(miniquad::gl::GL_SCISSOR_TEST);
        let screen_h = screen_height() as i32;
        miniquad::gl::glScissor(
            vis_x,
            screen_h - vis_y - vis_h,  // flip y
            vis_w,
            vis_h,
        );
    }

    set_camera(&Camera3D {
        position: vec3(0.0, 2.0, 5.0),
        target: vec3(0.0, 0.0, 0.0),
        up: vec3(0.0, 1.0, 0.0),
        viewport: Some((vis_x, vis_y, vis_w, vis_h)),
        ..Default::default()
    });

    set_default_camera();

    unsafe { // I hate opengl so much
        miniquad::gl::glDisable(miniquad::gl::GL_SCISSOR_TEST);
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

fn draw_library(x: f32, y: f32, w: f32, h: f32) {
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
    let label = truncate_to_fit(&label, 160.0, font_size);

    let text_y = *y + font_size as f32;

    // Check click on this row
    let mouse = mouse_position();
    let row_x = x + 5.0;
    let row_w = 160.0;
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
            player::load_song(&song.path);
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

fn draw_fast_forward_button(w: f32, h: f32) -> bool {
    if draw_button_rect((w/2.0)+20.0, h - 90.0, 40.0, 40.0, ">>", LIGHTGRAY, SIDEBAR_BG_COLOR, BLACK, BLACK, BLACK) {
        player::fast_forward_playback();
    }
    false
}

fn draw_rewind_button(w: f32, h: f32) -> bool {
    if draw_button_rect((w/2.0)-60.0, h - 90.0, 40.0, 40.0, "<<", LIGHTGRAY, SIDEBAR_BG_COLOR, BLACK, BLACK, BLACK) {
        player::rewind_playback();
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
    println!("Loading assets...");
    match load_texture("assets/images/Internet_Example.png").await {
        Ok(tex) => { LOGO_TEXTURE.set(tex).ok(); println!("Texture loaded"); }
        Err(e) => { eprintln!("Failed to load texture: {}", e); }
    }
    match load_ttf_font("assets/fonts/DejaVuSansMono.ttf").await {
        Ok(font) => { FONT.set(font).ok(); println!("Font loaded"); }
        Err(e) => { eprintln!("Failed to load font: {}", e); }
    }
}
