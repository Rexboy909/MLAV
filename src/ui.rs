use macroquad::prelude::*;
use once_cell::sync::OnceCell;

//--other statics--//
static LOGO_TEXTURE: OnceCell<Texture2D> = OnceCell::new();

//--colors--//

const BG_COLOR: Color = Color::new(53.0/255.0, 16.0/255.0, 108.0/255.0, 1.0);
const SIDEBAR_BG_COLOR: Color = Color::new(100.0/255.0, 100.0/255.0, 100.0/255.0, 1.0);

//--main ui drawing function--//
pub async fn draw_main_ui() {

    let w = screen_width().max(1110.0);
    let h = screen_height().max(683.0);

    clear_background(BG_COLOR);
    
    // --sidebar--//
    // for some reason, there is already a 30px margin?, so add another 30.0
    draw_rectangle(30.0, 30.0, 120.0, h - 180.0, SIDEBAR_BG_COLOR);
    draw_rectangle(30.0, h - 120.0, w - 60.0, 90.0, SIDEBAR_BG_COLOR);
    if draw_button_rect(30.0, 30.0, 120.0, 40.0, "Play", LIGHTGRAY, SIDEBAR_BG_COLOR, BLACK, BLACK) {
        println!("Play button clicked");
    }
    draw_visualizer(w,h);
    //println!("Screen dimensions: {}x{}", w, h);
}
fn draw_visualizer(w: f32, _h: f32) { // for now only a photo
    if let Some(tex) = LOGO_TEXTURE.get() {
        let scale = 1.0;
        draw_texture_ex(tex, w - tex.width() - 30.0   , 30.0, WHITE, DrawTextureParams {
            dest_size: Some(vec2(tex.width() * scale, tex.height() * scale)),
            ..Default::default()
        });
    }
}

fn draw_button_rect(x: f32, y: f32, w: f32, h: f32, label: &str, ch: Color, c: Color, tch: Color, tc: Color) -> bool {
    let mouse = mouse_position();
    let hovered = mouse.0 > x && mouse.0 < x + w && mouse.1 > y && mouse.1 < y + h;
    let clicked = hovered && is_mouse_button_pressed(MouseButton::Left);

    // draw button
    let color = if hovered { ch } else { c };
    let text_color = if hovered { tch } else { tc };
    draw_rectangle(x, y, w, h, color);
    draw_text(label, x + 10.0, y + h / 2.0 + 8.0, 24.0, text_color);

    clicked // returns true if clicked this frame
}

pub async fn load_assets() {
    match load_texture("assets/images/Internet_Example.png").await {
        Ok(tex) => { LOGO_TEXTURE.set(tex).ok(); }
        Err(e) => { eprintln!("Failed to load texture: {}", e); }
    }
}
