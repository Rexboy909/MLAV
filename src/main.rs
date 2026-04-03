mod ui;
mod player;
mod library;
mod lyrics;
mod song;

use macroquad::prelude::*;

#[macroquad::main(window_conf)]
async fn main() {
    library::init("Library/");
    library::watch();
    player::init();
    ui::load_assets().await;

    loop {
        ui::draw_main_ui();
        next_frame().await;
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "MLAV".to_owned(),
        window_width: 1280,
        window_height: 720,
        window_resizable: true,
        ..Default::default()
    }
}