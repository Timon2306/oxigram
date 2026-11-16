//! Start with the system. Windows: HKCU Run value "Oxigram"; Linux: an XDG
//! autostart .desktop file; macOS: a LaunchAgent. All start the exe with
//! `--autostart`, which keeps the window in the tray.

pub const FLAG: &str = "--autostart";

/// Whether this launch came from the system's autostart.
pub fn launched_by_system() -> bool {
    std::env::args().any(|a| a == FLAG)
}

/// Writes or removes the entry; errors are ignored (nothing sensible to do).
pub fn set(on: bool) {
    let Ok(exe) = std::env::current_exe() else { return };
    if on { imp::write(&exe) } else { imp::remove() }
}

/// If the setting is on and the entry is missing or points at another exe,
/// rewrite it.
pub fn sync(on: bool) {
    if !on {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    if imp::current().as_deref() != Some(imp::command(&exe).as_str()) {
        imp::write(&exe);
    }
}

#[cfg(windows)]
mod imp {
    use std::path::Path;

    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows::core::{PCWSTR, w};

    const KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    const NAME: PCWSTR = w!("Oxigram");

    pub fn command(exe: &Path) -> String {
        format!("\"{}\" {}", exe.display(), super::FLAG)
    }

    pub fn write(exe: &Path) {
        let value: Vec<u16> = command(exe).encode_utf16().chain(std::iter::once(0)).collect();
        let bytes: Vec<u8> = value.iter().flat_map(|u| u.to_le_bytes()).collect();
        unsafe {
            let mut key = HKEY::default();
            if RegCreateKeyExW(
                HKEY_CURRENT_USER,
                KEY,
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
            let _ = RegSetValueExW(key, NAME, None, REG_SZ, Some(&bytes));
            let _ = RegCloseKey(key);
        }
    }

    pub fn remove() {
        unsafe {
            let mut key = HKEY::default();
            if RegOpenKeyExW(HKEY_CURRENT_USER, KEY, None, KEY_SET_VALUE, &mut key).is_ok() {
                let _ = RegDeleteValueW(key, NAME);
                let _ = RegCloseKey(key);
            }
        }
    }

    pub fn current() -> Option<String> {
        unsafe {
            let mut key = HKEY::default();
            RegOpenKeyExW(HKEY_CURRENT_USER, KEY, None, KEY_READ, &mut key).ok().ok()?;
            let mut buf = vec![0u8; 4096];
            let mut len = buf.len() as u32;
            let r = RegQueryValueExW(key, NAME, None, None, Some(buf.as_mut_ptr()), Some(&mut len));
            let _ = RegCloseKey(key);
            r.ok().ok()?;
            let units: Vec<u16> =
                buf[..len as usize].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            Some(String::from_utf16_lossy(&units).trim_end_matches('\0').to_owned())
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::path::{Path, PathBuf};

    fn file() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("autostart").join("oxigram.desktop"))
    }

    /// Exec line value; quoted so spaces in the path survive.
    pub fn command(exe: &Path) -> String {
        format!("\"{}\" {}", exe.display(), super::FLAG)
    }

    pub fn write(exe: &Path) {
        let Some(path) = file() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text = format!(
            "[Desktop Entry]\nType=Application\nName=Oxigram\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            command(exe)
        );
        let _ = std::fs::write(path, text);
    }

    pub fn remove() {
        if let Some(path) = file() {
            let _ = std::fs::remove_file(path);
        }
    }

    pub fn current() -> Option<String> {
        let text = std::fs::read_to_string(file()?).ok()?;
        text.lines().find_map(|l| l.strip_prefix("Exec=")).map(str::to_owned)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::{Path, PathBuf};

    const LABEL: &str = "app.oxigram.desktop";

    fn file() -> Option<PathBuf> {
        Some(dirs::home_dir()?.join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
    }

    /// The program arguments as one comparable string.
    pub fn command(exe: &Path) -> String {
        format!("{} {}", exe.display(), super::FLAG)
    }

    pub fn write(exe: &Path) {
        let Some(path) = file() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let esc = exe.display().to_string().replace('&', "&amp;").replace('<', "&lt;");
        let text = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>{LABEL}</string>\n<key>ProgramArguments</key><array><string>{esc}</string><string>{}</string></array>\n<key>RunAtLoad</key><true/>\n</dict></plist>\n",
            super::FLAG
        );
        let _ = std::fs::write(path, text);
    }

    pub fn remove() {
        if let Some(path) = file() {
            let _ = std::fs::remove_file(path);
        }
    }

    pub fn current() -> Option<String> {
        let text = std::fs::read_to_string(file()?).ok()?;
        let mut strings = text.split("<string>").skip(2).filter_map(|s| s.split("</string>").next());
        let exe = strings.next()?.replace("&lt;", "<").replace("&amp;", "&");
        Some(format!("{exe} {}", strings.next()?))
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod imp {
    use std::path::Path;

    pub fn command(_: &Path) -> String {
        String::new()
    }
    pub fn write(_: &Path) {}
    pub fn remove() {}
    pub fn current() -> Option<String> {
        None
    }
}
