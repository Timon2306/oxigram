//! Tray icon: left click restores the window, the menu opens it, mutes every
//! notification or quits. Closing the window hides it to the tray (as the
//! Tauri build does); without a tray icon, closing quits.
//!
//! Windows and macOS: the icon lives on the UI thread. Linux: tray-icon needs
//! a GTK main loop, which runs on its own thread; if GTK cannot start there is
//! no tray and the app behaves like a plain window. Menu and click events are
//! polled from a slint timer on every platform.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use slint::ComponentHandle;
use slint::winit_030::{WinitWindowAccessor, winit};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::{AppWindow, notify, settings, with_app};

const POLL: Duration = Duration::from_millis(100);
/// The unread count is looked up once per this many polls.
const UNREAD_EVERY: u32 = 10;
const ICON: &[u8] = include_bytes!("../../src-tauri/icons/32x32.png");

/// Set once the icon is up; without it closing the window must quit.
static AVAILABLE: AtomicBool = AtomicBool::new(false);
/// The tooltip the tray should show; the Linux GTK thread reads it.
static TOOLTIP: Mutex<String> = Mutex::new(String::new());

pub fn available() -> bool {
    AVAILABLE.load(Ordering::Relaxed)
}

fn mute_label(mute: bool) -> &'static str {
    if mute { "Включить все уведомления" } else { "Выключить все уведомления" }
}

fn tooltip(unread: i32) -> String {
    match unread {
        n if n > 0 => format!("Oxigram — {n} непрочитанных"),
        _ => "Oxigram".to_owned(),
    }
}

/// The icon and its menu item whose label follows the mute flag.
struct Tray {
    icon: TrayIcon,
    mute: MenuItem,
    label: bool,
    tip: String,
}

impl Tray {
    fn new() -> Option<Self> {
        let image = image::load_from_memory(ICON).ok()?.into_rgba8();
        let (w, h) = image.dimensions();
        let icon = Icon::from_rgba(image.into_raw(), w, h).ok()?;
        let label = notify::mute_all();
        let open = MenuItem::with_id("open", "Открыть Oxigram", true, None);
        let mute = MenuItem::with_id("mute", mute_label(label), true, None);
        let quit = MenuItem::with_id("quit", "Выход", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &mute, &PredefinedMenuItem::separator(), &quit]).ok()?;
        let icon = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("Oxigram")
            .with_menu(Box::new(menu))
            // Left click opens the window; the menu is for the right button.
            .with_menu_on_left_click(false)
            .build()
            .ok()?;
        Some(Self { icon, mute, label, tip: "Oxigram".into() })
    }

    /// Brings label and tooltip in line with the shared state.
    fn sync(&mut self) {
        let mute = notify::mute_all();
        if mute != self.label {
            self.label = mute;
            self.mute.set_text(mute_label(mute));
        }
        let tip = TOOLTIP.lock().map(|t| t.clone()).unwrap_or_default();
        if !tip.is_empty() && tip != self.tip {
            let _ = self.icon.set_tooltip(Some(&tip));
            self.tip = tip;
        }
    }
}

#[cfg(not(target_os = "linux"))]
thread_local! {
    static TRAY: std::cell::RefCell<Option<Tray>> = const { std::cell::RefCell::new(None) };
}

fn start_icon() {
    #[cfg(not(target_os = "linux"))]
    if let Some(tray) = Tray::new() {
        TRAY.with(|t| *t.borrow_mut() = Some(tray));
        AVAILABLE.store(true, Ordering::Relaxed);
    }
    #[cfg(target_os = "linux")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("tray".into()).stack_size(512 * 1024).spawn(move || {
            if gtk::init().is_err() {
                let _ = tx.send(false);
                return;
            }
            let Some(mut tray) = Tray::new() else {
                let _ = tx.send(false);
                return;
            };
            let _ = tx.send(true);
            gtk::glib::timeout_add_local(Duration::from_millis(200), move || {
                tray.sync();
                gtk::glib::ControlFlow::Continue
            });
            gtk::main();
        });
        if spawned.is_ok() && rx.recv_timeout(Duration::from_secs(3)).unwrap_or(false) {
            AVAILABLE.store(true, Ordering::Relaxed);
        }
    }
}

fn sync_icon() {
    #[cfg(not(target_os = "linux"))]
    TRAY.with(|t| {
        if let Some(tray) = t.borrow_mut().as_mut() {
            tray.sync();
        }
    });
}

/// Shows, restores and focuses the window.
pub fn show_window(ui: &AppWindow) {
    let window = ui.window();
    let _ = ui.show();
    window.set_minimized(false);
    window.with_winit_window(|w| {
        w.set_visible(true);
        w.focus_window();
    });
}

fn toggle_mute() {
    let on = !notify::mute_all();
    notify::set_mute_all(on);
    with_app(|app| {
        app.settings.mute_all = on;
        app.settings.save();
    });
    sync_icon();
}

/// Creates the tray, hooks window close and focus, and starts the poll timer.
/// The returned timer must be kept alive for as long as the loop runs.
pub fn init(ui: &AppWindow) -> slint::Timer {
    start_icon();

    ui.window().on_close_requested(|| {
        if !available() {
            let _ = slint::quit_event_loop();
        }
        // Hidden, not destroyed: the event loop keeps running for the tray.
        slint::CloseRequestResponse::HideWindow
    });
    ui.window().on_winit_window_event(|_, event| {
        if let winit::event::WindowEvent::Focused(on) = event {
            notify::set_focused(*on);
        }
        slint::winit_030::EventResult::Propagate
    });

    let weak = ui.as_weak();
    let mut tick = 0u32;
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, POLL, move || {
        let Some(ui) = weak.upgrade() else { return };
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id.as_ref() {
                "open" => show_window(&ui),
                "mute" => toggle_mute(),
                "quit" => {
                    let _ = slint::quit_event_loop();
                }
                _ => {}
            }
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(&ui);
            }
        }
        let window = ui.window();
        notify::set_shown(window.is_visible() && !window.is_minimized());
        if tick % UNREAD_EVERY == 0 {
            let unread = with_app(|app| app.unread_total()).unwrap_or(0);
            if let Ok(mut tip) = TOOLTIP.lock() {
                *tip = tooltip(unread);
            }
        }
        tick = tick.wrapping_add(1);
        sync_icon();
    });
    timer
}

/// Start-up hook: registers the notifier with the core.
pub fn install_notifier(ui: &oxigram_core::updates::UiSink) {
    notify::install(ui, settings::Settings::load().mute_all);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_state() {
        assert_eq!(mute_label(false), "Выключить все уведомления");
        assert_eq!(mute_label(true), "Включить все уведомления");
        assert_eq!(tooltip(0), "Oxigram");
        assert!(tooltip(3).contains('3'));
    }

    #[test]
    fn icon_decodes() {
        assert!(image::load_from_memory(ICON).is_ok());
    }
}
