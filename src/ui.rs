use macroquad::prelude::*;
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::library::{self, LibraryNode};
use crate::player;

static IS_PLAYING: AtomicBool = AtomicBool::new(false);

//--other statics--//
static LOGO_TEXTURE: OnceCell<Texture2D> = OnceCell::new();
static FONT: OnceCell<Font> = OnceCell::new();

//--colors--//

const BG_COLOR: Color = Color::new(53.0/255.0, 16.0/255.0, 108.0/255.0, 1.0);
const SIDEBAR_BG_COLOR: Color = Color::new(100.0/255.0, 100.0/255.0, 100.0/255.0, 1.0);

//--main ui drawing function--//
pub fn draw_main_ui() {

    let w = screen_width().max(1110.0);
    let h = screen_height().max(683.0);

    clear_background(BG_COLOR);
    
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
    draw_visualizer(w,h);
    //println!("Screen dimensions: {}x{}", w, h);
}

fn draw_visualizer(w: f32, _h: f32) { // for now only a photo
    if let Some(tex) = LOGO_TEXTURE.get() {
        let scale = 1.0;
        draw_texture_ex(tex, w - tex.width() - 60.0   , 30.0, WHITE, DrawTextureParams {
            dest_size: Some(vec2(tex.width() * scale, tex.height() * scale)),
            ..Default::default()
        });
    }
}

fn draw_library(x: f32, y: f32, w: f32, h: f32) {
    let mut current_y = y + 10.0;
    library::with_library(|root| {
        if let Some(node) = root {
            draw_node(node, 0, x, y + h, y, &mut current_y, true, &vec![]);
        }
    });
}

fn draw_node(node: &LibraryNode, depth: u32, x: f32, max_y: f32, min_y: f32, y: &mut f32, is_last: bool, open_depths: &Vec<bool>) {
    if *y > max_y { return; }

    let font_size = 16;
    let line_height = 20.0;

    let mut prefix = String::new();
    for i in 0..depth as usize {
        if i == depth as usize - 1 {
            if is_last {
                prefix.push_str("└─ ");
            } else {
                prefix.push_str("├─ ");
            }
        } else if open_depths[i] {
            prefix.push_str("│  ");
        } else {
            prefix.push_str("   ");
        }
    }

    let (label, color) = match node {
        LibraryNode::Folder { name, .. } => (format!("{}{}/", prefix, name), WHITE),
        LibraryNode::Track(song) => (format!("{}{}", prefix, song.title), LIGHTGRAY),
    };

    draw_text_ex(&label, x + 5.0, *y + font_size as f32, TextParams {
        font: FONT.get(),
        font_size,
        color,
        ..Default::default()
    });
    *y += line_height;

    if let LibraryNode::Folder { children, .. } = node {
        let len = children.len();
        let mut new_depths = open_depths.clone();
        new_depths.push(!is_last);
        for (i, child) in children.iter().enumerate() {
            draw_node(child, depth + 1, x, max_y, min_y, y, i == len - 1, &new_depths);
        }
    }
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
    // add ellipsis if we truncated
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
