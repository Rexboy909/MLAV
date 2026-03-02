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

    draw_visualizer(w,h);
    println!("Screen dimensions: {}x{}", w, h);
}
fn draw_visualizer(w: f32, _h: f32) {
    if let Some(tex) = LOGO_TEXTURE.get() {
        let scale = 1.0;
        draw_texture_ex(tex, w - tex.width() - 30.0   , 30.0, WHITE, DrawTextureParams {
            dest_size: Some(vec2(tex.width() * scale, tex.height() * scale)),
            ..Default::default()
        });
    }
}

pub async fn load_assets() {
    match load_texture("assets/images/Internet_Example.png").await {
        Ok(tex) => { LOGO_TEXTURE.set(tex).ok(); }
        Err(e) => { eprintln!("Failed to load texture: {}", e); }
    }
}