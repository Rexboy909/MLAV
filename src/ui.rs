use macroquad::prelude::*;

struct Colors {
    bg_color: Color,
    element_color: Color,
}
/*let ui_colors = Colors {
    bg_color: BLACK,
    element_color: GRAY,
};
*/
pub fn draw_main_ui() {

    clear_background(BLACK);
    
    // --sidebar--//
    // for some reason, there is already a 30px margin?, so add another 30.0
    draw_rectangle(30.0, 30.0, 120.0, screen_height() - 60.0, LIGHTGRAY); 
    //draw_rectangle_lines(x, y, w, h, thickness, color); // doubled up for rectangle border
}