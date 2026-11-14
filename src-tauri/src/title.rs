//! "Oxigram — 64 МБ ОЗУ" in the window title, measured by `oxigram_core::mem`.

use oxigram_core::mem::usage;

/// Unread messages in unmuted chats, as last reported by the window.
pub static UNREAD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// How often the title is refreshed while the window is on screen.
const TITLE_EVERY: std::time::Duration = std::time::Duration::from_secs(3);

/// Keeps "Oxigram — 64 МБ ОЗУ" in the main window's title. A plain thread
/// that sleeps between samples; it skips the work while the window is
/// minimized or closed to the tray.
pub fn show_in_title<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    use tauri::Manager;
    std::thread::spawn(move || {
        let mut shown = String::new();
        loop {
            std::thread::sleep(TITLE_EVERY);
            let Some(window) = app.get_webview_window("main") else {
                shown.clear();
                continue;
            };
            if !window.is_visible().unwrap_or(false) || window.is_minimized().unwrap_or(true) {
                continue;
            }
            let Some(info) = usage() else { continue };
            let unread = UNREAD.load(std::sync::atomic::Ordering::Relaxed);
            let count = if unread > 0 { format!(" ({unread})") } else { String::new() };
            let title = format!("Oxigram{count} — {} МБ ОЗУ", info.bytes / (1024 * 1024));
            if title != shown && window.set_title(&title).is_ok() {
                shown = title;
            }
        }
    });
}

