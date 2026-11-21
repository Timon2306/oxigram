//! Timings of slow steps (sign-out, connecting, QR, the first dialogs) on
//! stderr when OXIGRAM_TIMING is set; nothing otherwise.

use std::sync::OnceLock;
use std::time::Instant;

fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("OXIGRAM_TIMING").is_some())
}

/// Prints "[timing] <what>: <ms> ms" since `since`.
pub fn mark(what: &str, since: Instant) {
    if on() {
        eprintln!("[timing] {what}: {} ms", since.elapsed().as_millis());
    }
}
