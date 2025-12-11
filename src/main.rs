mod ui;
use macroquad::prelude::*;

#[macroquad::main("MLAV")]
async fn main() {
    clear_background(BLACK);
    loop {
        ui::draw_main_ui();

        next_frame().await;
    }
}
