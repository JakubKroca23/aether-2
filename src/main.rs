mod audio;
mod gfx;
mod view;

use macroquad::prelude::*;

/// `rand` / `getrandom` have no OS entropy on wasm32-unknown-unknown.
/// A custom filler keeps the wasm module free of wasm-bindgen so the
/// stock Macroquad loader can instantiate it. The sim itself seeds
/// `StdRng` from a u64 and does not need this for gameplay.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn wasm_fill_entropy(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    use std::cell::Cell;
    thread_local! {
        static STATE: Cell<u64> = const { Cell::new(0) };
    }
    STATE.with(|cell| {
        let mut state = cell.get();
        if state == 0 {
            state = aether::unix_millis().wrapping_mul(0x9E37_79B9_7F4A_7C15);
            if state == 0 {
                state = 0xA37E_5EED_C0FF_EE11;
            }
        }
        for chunk in buf.chunks_mut(8) {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let bytes = state.to_le_bytes();
            for (slot, byte) in chunk.iter_mut().zip(bytes) {
                *slot = byte;
            }
        }
        cell.set(state);
    });
    Ok(())
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
getrandom::register_custom_getrandom!(wasm_fill_entropy);

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
