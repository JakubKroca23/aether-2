//! Browser hooks imported by the miniquad loader (`web/aether_host.js`).
//!
//! These are plain `env` imports so the wasm module stays free of wasm-bindgen.
//! The simulation library still does not link Macroquad.

#[link(wasm_import_module = "env")]
extern "C" {
    fn aether_unix_ms() -> f64;
    fn aether_auto_run() -> i32;
    fn aether_storage_len() -> i32;
    fn aether_storage_read(ptr: *mut u8, cap: i32) -> i32;
    fn aether_storage_write(ptr: *const u8, len: i32) -> i32;
}

/// Milliseconds since the Unix epoch, from `Date.now()`.
pub fn unix_millis() -> u64 {
    unsafe { aether_unix_ms().max(0.0) as u64 }
}

/// `true` when the page was opened with `?run=1` (skip the lobby).
pub fn auto_run() -> bool {
    unsafe { aether_auto_run() != 0 }
}

const MAX_BLOB: i32 = 8 * 1024 * 1024;

pub fn read_blob() -> Option<Vec<u8>> {
    unsafe {
        let len = aether_storage_len();
        if len <= 0 || len > MAX_BLOB {
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        let n = aether_storage_read(buf.as_mut_ptr(), len);
        if n <= 0 {
            return None;
        }
        let n = (n as usize).min(buf.len());
        buf.truncate(n);
        Some(buf)
    }
}

pub fn write_blob(bytes: &[u8]) -> bool {
    if bytes.len() > MAX_BLOB as usize {
        return false;
    }
    unsafe { aether_storage_write(bytes.as_ptr(), bytes.len() as i32) != 0 }
}
