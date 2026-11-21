//! Updates: a quiet "↑ 0.3.1" badge when a newer release is published, and
//! in "О программе" the check, the download with its progress and the
//! install. The request and the download are in `oxigram_core::update`
//! (through the user's proxy, SHA-256 checked); here are the window and the
//! hand-over to the installer.

use std::cell::RefCell;
use std::path::Path;
#[cfg(all(unix, not(target_os = "macos")))]
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use oxigram_core::update::{self, Release};

use crate::{AppWindow, format, run, tg, ui, with_app};

/// At most one automatic check per day.
const EVERY: i64 = 24 * 60 * 60;

thread_local! {
    static FOUND: RefCell<Option<Release>> = const { RefCell::new(None) };
}

pub fn wire(window: &AppWindow) {
    window.on_update_check(|| check(true));
    window.on_update_install(install);
    window.on_update_notes(|| {
        if let Some(page) = FOUND.with(|f| f.borrow().as_ref().map(|r| r.page.clone())) {
            crate::links::open_url(&page);
        }
    });
    window.on_open_update(|| {
        if let Some(ui) = ui() {
            crate::settings::fill(&ui);
            ui.set_settings_page("about".into());
            ui.set_settings_open(true);
        }
    });
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The automatic check at start-up, when it is on and a day has passed.
pub fn at_start() {
    let due = with_app(|app| app.settings.auto_update && now() - app.settings.update_checked >= EVERY).unwrap_or(false);
    if due {
        check(false);
    }
}

fn status(text: &str) {
    if let Some(ui) = ui() {
        ui.set_update_status(text.into());
    }
}

/// Asks GitHub; `manual` is the button (it reports "no update" too).
fn check(manual: bool) {
    if let Some(ui) = ui() {
        if ui.get_update_busy() {
            return;
        }
        if manual {
            ui.set_update_busy(true);
            ui.set_update_status("Checking…".into());
        }
    }
    let proxy = tg().proxy();
    run(
        async move { update::check(&proxy, env!("CARGO_PKG_VERSION")).await },
        move |result| {
            with_app(|app| {
                app.settings.update_checked = now();
                app.settings.save();
            });
            let Some(ui) = ui() else { return };
            if manual {
                ui.set_update_busy(false);
            }
            match result {
                Ok(Some(release)) => {
                    ui.set_update_version(release.version.clone().into());
                    ui.set_update_status(format!("Version {} available", release.version).into());
                    FOUND.with(|f| *f.borrow_mut() = Some(release));
                }
                Ok(None) => {
                    if manual {
                        ui.set_update_status("You have the latest version".into());
                    }
                }
                // A quiet check stays quiet when GitHub is unreachable.
                Err(err) if manual => ui.set_update_status(format!("Couldn't check: {}", format::error(&err)).into()),
                Err(_) => {}
            }
        },
    );
}

/// "Обновить": download, check, then hand over to the installer.
fn install() {
    let Some(release) = FOUND.with(|f| f.borrow().clone()) else { return };
    {
        let Some(window) = ui() else { return };
        if window.get_update_busy() {
            return;
        }
    }
    // No installer for this system in the release: its page.
    let Some(asset) = release.asset.clone() else {
        crate::links::open_url(&release.page);
        return;
    };
    if let Some(window) = ui() {
        window.set_update_busy(true);
        window.set_update_progress(0.0);
        window.set_update_status("Downloading…".into());
    }
    let dir = std::env::temp_dir().join("oxigram-update");
    let proxy = tg().proxy();
    let total = asset.size;
    let last = Arc::new(AtomicI32::new(-1));
    run(
        async move {
            update::download(&proxy, &release, &dir, move |bytes| {
                let percent = if total > 0 { (bytes.min(total) * 100 / total) as i32 } else { 0 };
                if last.swap(percent, Ordering::Relaxed) == percent {
                    return;
                }
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui() {
                        ui.set_update_progress(percent as f32 / 100.0);
                        ui.set_update_status(
                            format!(
                                "Downloading… {} of {}",
                                crate::menus::format_size(bytes),
                                crate::menus::format_size(total)
                            )
                            .into(),
                        );
                    }
                });
            })
            .await
        },
        |result| {
            let Some(ui) = ui() else { return };
            ui.set_update_busy(false);
            match result {
                Ok(path) => hand_over(&path),
                Err(err) if err.code == "UPDATE_CHECKSUM" => {
                    ui.set_update_status("The file is corrupted (checksum mismatch), update cancelled".into())
                }
                Err(err) if err.code == "UPDATE_NO_SUMS" => {
                    ui.set_update_status("The release has no checksums, update cancelled".into())
                }
                Err(err) => ui.set_update_status(format!("Couldn't download: {}", format::error(&err)).into()),
            }
        },
    );
}

/// Runs the checked installer.
fn hand_over(path: &Path) {
    #[cfg(windows)]
    {
        // The NSIS installer replaces the program; Oxigram quits so its exe
        // is free, and the installer starts it again at the end.
        if open_file(path) {
            status("Starting the installer…");
            let _ = slint::quit_event_loop();
        } else {
            status("Couldn't start the installer");
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(target) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
            match replace_appimage(path, &target) {
                Ok(()) => {
                    let _ = std::process::Command::new(&target).spawn();
                    let _ = slint::quit_event_loop();
                }
                Err(err) => status(&format!("Couldn't replace the AppImage: {err}")),
            }
        } else if open_file(path) {
            status("Open the package and install it");
        } else {
            status(&format!("Package downloaded: {}", path.display()));
        }
    }
    #[cfg(target_os = "macos")]
    {
        if open_file(path) {
            status("Drag Oxigram to Applications, then restart");
        } else {
            status(&format!("Image downloaded: {}", path.display()));
        }
    }
}

/// The new AppImage takes the place of the running one (Linux keeps the
/// running file's data alive until it exits).
#[cfg(all(unix, not(target_os = "macos")))]
fn replace_appimage(new: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let staged: PathBuf = target.with_extension("new");
    std::fs::copy(new, &staged)?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(&staged, target)?;
    let _ = std::fs::remove_file(new);
    Ok(())
}

/// Opens a file with the system (runs an installer, mounts a disk image).
fn open_file(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::{PCWSTR, w};
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let code = unsafe { ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), None, None, SW_SHOWNORMAL) };
        code.0 as usize > 32
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn().is_ok()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn().is_ok()
    }
}
