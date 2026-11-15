//! System notifications for new incoming messages. The core decides when one
//! is due (not for muted chats, not for the chat being looked at); this module
//! only answers "is the user looking?" and shows the toast.

use std::sync::atomic::{AtomicBool, Ordering};

use oxigram_core::updates::Notifier;

/// The window is on screen and not minimized (kept by the tray's timer).
static SHOWN: AtomicBool = AtomicBool::new(false);
/// The window has keyboard focus (kept by winit's focus events).
static FOCUSED: AtomicBool = AtomicBool::new(false);
/// "Выключить все уведомления".
static MUTE_ALL: AtomicBool = AtomicBool::new(false);

pub fn set_shown(on: bool) {
    SHOWN.store(on, Ordering::Relaxed);
}

pub fn set_focused(on: bool) {
    FOCUSED.store(on, Ordering::Relaxed);
}

pub fn mute_all() -> bool {
    MUTE_ALL.load(Ordering::Relaxed)
}

/// Updates the flag and the tray menu label (call on the UI thread).
pub fn set_mute_all(on: bool) {
    MUTE_ALL.store(on, Ordering::Relaxed);
}

pub struct SystemNotifier;

impl Notifier for SystemNotifier {
    fn looking(&self) -> bool {
        SHOWN.load(Ordering::Relaxed) && FOCUSED.load(Ordering::Relaxed)
    }

    fn show(&self, title: String, body: String) {
        if mute_all() {
            return;
        }
        // Errors (no notification service, Windows 7) are not worth a crash.
        let _ = notify_rust::Notification::new()
            .appname("Oxigram")
            .summary(&title)
            .body(&body)
            .show();
    }
}

/// Registers the notifier with the core; once at start-up.
pub fn install(ui: &oxigram_core::updates::UiSink, mute_all: bool) {
    MUTE_ALL.store(mute_all, Ordering::Relaxed);
    ui.set_notifier(Box::new(SystemNotifier));
}
