use macroquad::prelude::*;

//--colors--//

const BG_COLOR: Color = Color::new(53.0/255.0, 16.0/255.0, 108.0/255.0, 1.0);
const SIDEBAR_BG_COLOR: Color = Color::new(100.0/255.0, 100.0/255.0, 100.0/255.0, 1.0);

//--main ui drawing function--//
pub fn draw_main_ui() {

    clear_background(BG_COLOR);
    
    // --sidebar--//
    // for some reason, there is already a 30px margin?, so add another 30.0
    draw_rectangle(30.0, 30.0, 120.0, screen_height() - 60.0, SIDEBAR_BG_COLOR); 
    //draw_rectangle_lines(x, y, w, h, thickness, color); // doubled up for rectangle border
}