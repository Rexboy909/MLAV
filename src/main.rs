use macroquad::prelude::*;

#[macroquad::main("MLAV")]
async fn main() {
    loop {

        draw_line(40.0, 40.0, 100.0, 200.0, 15.0, BLUE);
        draw_rectangle(screen_width() / 2.0 - 60.0, 100.0, 120.0, 60.0, GREEN);

        draw_text("Hello, Macroquad!", 20.0, 20.0, 30.0, DARKGRAY);

        next_frame().await;

        if is_mouse_button_down(MouseButton::Left){
            clear_background(BLUE);
        } else {
            clear_background(RED);
        }
    }
}
