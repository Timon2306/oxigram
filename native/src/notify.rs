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
        let mut n = notify_rust::Notification::new();
        n.appname("Oxigram").summary(&title).body(&body);
        #[cfg(windows)]
        n.app_id(APP_USER_MODEL_ID);
        let _ = n.show();
    }
}

/// Registers the notifier with the core; once at start-up.
pub fn install(ui: &oxigram_core::updates::UiSink, mute_all: bool) {
    #[cfg(windows)]
    win_identity::register();
    MUTE_ALL.store(mute_all, Ordering::Relaxed);
    ui.set_notifier(Box::new(SystemNotifier));
}

/// Same as APP_ID in main.rs and the installer's shortcut.
#[cfg(windows)]
const APP_USER_MODEL_ID: &str = "app.oxigram.desktop";

/// Windows shows a toast under the name registered for its AppUserModelID.
/// The installer's Start-menu shortcut does it for installed copies; this also
/// covers a portable exe.
#[cfg(windows)]
mod win_identity {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegSetValueExW,
    };
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    use windows::core::{HSTRING, PCWSTR, w};

    pub fn register() {
        unsafe {
            let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(super::APP_USER_MODEL_ID));
            let mut key = HKEY::default();
            let path = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{}", super::APP_USER_MODEL_ID));
            if RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &path,
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
            .is_err()
            {
                return;
            }
            let name: Vec<u8> = "Oxigram\0".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
            let _ = RegSetValueExW(key, w!("DisplayName"), None, REG_SZ, Some(&name));
            let _ = RegCloseKey(key);
        }
    }
}
