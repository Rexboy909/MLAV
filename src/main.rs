mod ui;
use macroquad::prelude::*;

#[macroquad::main(window_conf)]
async fn main() {

    ui::load_assets().await;

    loop {
        ui::draw_main_ui().await;
        next_frame().await;
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "MLAV".to_owned(),
        window_width: 1280,
        window_height: 720,
        window_resizable: true,
        platform: miniquad::conf::Platform {
            swap_interval: Some(0),
            ..Default::default()
        },
        ..Default::default()
    }
}
