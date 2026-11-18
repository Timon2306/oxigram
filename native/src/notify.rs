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

    fn show(&self, title: String, body: String, avatar: Option<(i64, String)>) {
        if mute_all() {
            return;
        }
        // With a passcode cached pictures are sealed; the system's
        // notification could not read them (and must not get a plain copy).
        let avatar = avatar.filter(|_| !crate::tg().passcode_on());
        let Some((peer, photo)) = avatar else {
            present(&title, &body, None);
            return;
        };
        // The avatar from the app's cache (downloaded in the small size only
        // when missing), waiting at most a moment before showing without it.
        let Some(runtime) = crate::RUNTIME.get() else {
            present(&title, &body, None);
            return;
        };
        runtime.spawn(async move {
            let fetch = oxigram_core::chats::avatar(crate::tg(), peer, &photo);
            let path = match tokio::time::timeout(AVATAR_WAIT, fetch).await {
                Ok(Ok(path)) => Some(std::path::PathBuf::from(path)),
                _ => None,
            };
            // Showing a toast blocks briefly (COM / D-Bus): not on the runtime.
            std::thread::spawn(move || present(&title, &body, path.as_deref()));
        });
    }
}

/// How long a toast waits for the chat's picture.
const AVATAR_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Shows the toast. Errors (no notification service) are not worth a crash.
fn present(title: &str, body: &str, avatar: Option<&std::path::Path>) {
    #[cfg(windows)]
    {
        use tauri_winrt_notification::{IconCrop, Toast};
        let mut toast = Toast::new(APP_USER_MODEL_ID).title(title).text1(body);
        if let Some(path) = avatar {
            toast = toast.icon(path, IconCrop::Circular, "");
        }
        let _ = toast.show();
    }
    #[cfg(not(windows))]
    {
        let mut n = notify_rust::Notification::new();
        n.appname("Oxigram").summary(title).body(body);
        // Linux shows a file path as the notification's icon.
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some(path) = avatar {
            n.icon(&path.to_string_lossy());
        }
        #[cfg(target_os = "macos")]
        let _ = avatar;
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

/// Windows shows a toast under the name and icon registered for its
/// AppUserModelID (HKCU, Software/Classes/AppUserModelId/<id>: DisplayName
/// and IconUri). Without IconUri the toast had a grey placeholder. IconUri
/// must be a file path, so the icon is written next to the app's data.
#[cfg(windows)]
mod win_identity {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegSetValueExW,
    };
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    use windows::core::{HSTRING, PCWSTR, w};

    /// The app icon shown on toasts.
    const ICON: &[u8] = include_bytes!("../icons/128x128.png");

    /// A REG_SZ value: UTF-16LE with the closing zero.
    fn wide(text: &str) -> Vec<u8> {
        text.encode_utf16().chain(Some(0)).flat_map(|u| u.to_le_bytes()).collect()
    }

    /// LOCALAPPDATA/Oxigram/toast-icon.png, rewritten when it differs.
    fn icon_file() -> Option<std::path::PathBuf> {
        let dir = dirs::data_local_dir()?.join("Oxigram");
        let path = dir.join("toast-icon.png");
        if std::fs::read(&path).ok().as_deref() != Some(ICON) {
            std::fs::create_dir_all(&dir).ok()?;
            std::fs::write(&path, ICON).ok()?;
        }
        Some(path)
    }

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
            let _ = RegSetValueExW(key, w!("DisplayName"), None, REG_SZ, Some(&wide("Oxigram")));
            if let Some(icon) = icon_file() {
                let _ = RegSetValueExW(key, w!("IconUri"), None, REG_SZ, Some(&wide(&icon.to_string_lossy())));
            }
            let _ = RegCloseKey(key);
        }
    }
}
