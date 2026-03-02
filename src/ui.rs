use macroquad::prelude::*;
use once_cell::sync::OnceCell;

//--other statics--//
static LOGO_TEXTURE: OnceCell<Texture2D> = OnceCell::new();

//--colors--//

const BG_COLOR: Color = Color::new(53.0/255.0, 16.0/255.0, 108.0/255.0, 1.0);
const SIDEBAR_BG_COLOR: Color = Color::new(100.0/255.0, 100.0/255.0, 100.0/255.0, 1.0);

//--main ui drawing function--//
pub async fn draw_main_ui() {

    clear_background(BG_COLOR);
    
    // --sidebar--//
    // for some reason, there is already a 30px margin?, so add another 30.0
    draw_rectangle(30.0, 30.0, 120.0, screen_height() - 60.0, SIDEBAR_BG_COLOR); 
    //draw_rectangle_lines(x, y, w, h, thickness, color); // doubled up for rectangle border
    if LOGO_TEXTURE.get().is_none() {
        match load_texture("assets/images/MLAV_LOGO-01.png").await {
            Ok(tex) => {
                LOGO_TEXTURE.set(tex).ok();
            },
            Err(e) => {
                println!("Failed to load texture: {}", e);
            }
        }
    }
    draw_visualizer();
    draw_circle(60.0, 60.0, 30.0, RED);
}
fn draw_visualizer() {
    println!("draw_visualizer called");
    if let Some(vs) = LOGO_TEXTURE.get() {
        draw_texture(vs, 300.0, 300.0, WHITE);
    } else {
        // Fallback: draw a rectangle and print debug info
        draw_rectangle(300.0, 300.0, 200.0, 200.0, YELLOW);
        draw_text("Image not loaded", 320.0, 400.0, 32.0, BLACK);
    }
}