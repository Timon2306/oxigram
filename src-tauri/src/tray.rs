//! Tray mode: closing the window destroys the WebView, the app lives on.
//!
//! The Telegram connection and the update task live in Rust state, so the
//! WebView (~65 MB of the footprint) is only needed while a window is on
//! screen. Closing the window drops it entirely; the tray icon brings a fresh
//! one back, and the frontend reloads its state from the backend.

use tauri::{
    AppHandle, Manager, Runtime,
    menu::{MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

/// Set once the icon is up; without it closing the window must quit.
static AVAILABLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn available() -> bool {
    AVAILABLE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Command-line flag for starting into the tray (used by autostart).
pub const MINIMIZED_ARG: &str = "--minimized";

/// Label of the main window, as given by the first entry in `tauri.conf.json`.
const MAIN: &str = "main";

/// Creates the one and only tray icon. Called once from `setup`.
pub fn init<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Открыть Oxigram", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let menu = MenuBuilder::new(app)
        .item(&open)
        .separator()
        .item(&quit)
        .build()?;

    let mut tray = TrayIconBuilder::with_id(MAIN)
        .tooltip("Oxigram")
        .menu(&menu)
        // Left click opens the window; the menu is for the right button only.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            // A real exit: `Some` code is not stopped by `prevent_exit`.
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    AVAILABLE.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Shows the unread count in the tray tooltip.
pub fn set_unread<R: Runtime>(app: &AppHandle<R>, unread: u32) {
    if let Some(tray) = app.tray_by_id(MAIN) {
        let tip = match unread {
            0 => "Oxigram".to_owned(),
            n => format!("Oxigram — {n} непрочитанных"),
        };
        let _ = tray.set_tooltip(Some(tip));
    }
}

/// Brings the main window to the front, recreating it (and its WebView) from
/// the config if it was closed. `from_config` keeps `additionalBrowserArgs`,
/// and the builder-level window-event hook applies to the new window as well.
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let Some(config) = app.config().app.windows.first() else {
        return;
    };
    // The config window starts hidden (see setup), so show this one.
    if let Ok(builder) = tauri::WebviewWindowBuilder::from_config(app, config) {
        if let Ok(window) = builder.build() {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}
