mod audio;
mod gfx;
mod view;

use macroquad::prelude::*;

fn window_conf() -> Conf {
    Conf {
        window_title: "Aether".to_owned(),
        window_width: 1280,
        window_height: 800,
        high_dpi: true,
        sample_count: 2,
        fullscreen: false,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    view::run().await;
}
