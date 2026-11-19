//! Files (documents) in messages: the card in the bubble, downloading into
//! Downloads/Oxigram with progress and cancel, opening with the system's
//! application, "Показать в папке" and "Сохранить как…".
//!
//! Nothing downloads by itself: only a click on the card or the menu starts
//! it. What downloads, and where it went, lives here (not in the rows, which
//! Slint recreates while scrolling); rows read it when they are built.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use oxigram_core::chats;
use oxigram_core::store::chats::MessageRow;
use oxigram_core::tr;
use slint::ComponentHandle;
use slint::winit_030::WinitWindowAccessor;

use crate::{FileItem, RUNTIME, format, tg, ui, with_app};

/// A download started in this run, or a file found already saved.
struct Job {
    /// 0..1 while downloading.
    progress: f32,
    /// Bytes so far, for "0,4 МБ из 1,2 МБ".
    bytes: u64,
    /// Where the file is once complete.
    path: Option<PathBuf>,
    /// Stops the download (its partial file is removed by the core).
    abort: Option<tokio::task::AbortHandle>,
}

thread_local! {
    static JOBS: RefCell<HashMap<(i64, i32), Job>> = RefCell::new(HashMap::new());
}

pub fn wire(ui: &crate::AppWindow) {
    ui.on_file_click(click);
}

/// Whether a message is shown as a file card.
pub fn is_file(m: &MessageRow) -> bool {
    m.kind == "document"
}

/// The card of a file message, as the row shows it now.
pub fn item(m: &MessageRow) -> FileItem {
    let name = display_name(m);
    let total = m.file_size.max(0) as u64;
    let key = (m.peer_id, m.id);
    let (state, progress, bytes) = JOBS.with(|jobs| {
        let mut jobs = jobs.borrow_mut();
        // A file saved in an earlier run is found by its name and size.
        if !jobs.contains_key(&key)
            && let Some(path) = default_dir().map(|dir| dir.join(safe_name(&name)))
            && std::fs::metadata(&path).is_ok_and(|meta| meta.is_file() && (total == 0 || meta.len() == total))
        {
            jobs.insert(key, Job { progress: 1.0, bytes: total, path: Some(path), abort: None });
        }
        match jobs.get(&key) {
            Some(job) if job.path.is_some() => (2, 1.0, total),
            Some(job) => (1, job.progress, job.bytes),
            None => (0, 0.0, 0),
        }
    });
    let size = match (state, total) {
        (1, 0) => format!("{} …", crate::menus::format_size(bytes)),
        (1, _) => tr!("{} of {}", crate::menus::format_size(bytes), crate::menus::format_size(total)),
        (_, 0) => String::new(),
        _ => crate::menus::format_size(total),
    };
    FileItem {
        id: m.id,
        ext: extension(&name).into(),
        name: name.into(),
        size: size.into(),
        state,
        progress,
    }
}

/// The file's own name, or "File".
fn display_name(m: &MessageRow) -> String {
    let name = m.file_name.trim();
    if name.is_empty() { tr!("File") } else { name.to_owned() }
}

/// "PDF", "ZIP"… (at most 4 letters), or an empty string.
fn extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() && ext.chars().count() <= 4 => ext.to_uppercase(),
        _ => String::new(),
    }
}

/// The name as the core saves it (characters Windows refuses replaced).
fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_owned()
}

/// Downloads/Oxigram.
fn default_dir() -> Option<PathBuf> {
    dirs::download_dir().map(|dir| dir.join("Oxigram"))
}

fn row_of(id: i32) -> Option<MessageRow> {
    with_app(|app| app.messages.iter().find(|m| m.id == id).cloned()).flatten()
}

/// Redraws the bubble of a message of the open chat.
fn refresh(peer: i64, id: i32) {
    with_app(|app| {
        if app.chat == Some(peer)
            && let Some(index) = app.messages.iter().position(|m| m.id == id)
        {
            app.refresh_row(index);
        }
    });
}

fn finished_path(key: (i64, i32)) -> Option<PathBuf> {
    JOBS.with(|jobs| jobs.borrow().get(&key).and_then(|job| job.path.clone()))
}

/// A click on the card: download, cancel the download, or open the file.
fn click(id: i32) {
    let Some(row) = row_of(id) else { return };
    let key = (row.peer_id, id);
    if let Some(path) = finished_path(key) {
        if path.is_file() {
            open(&path);
            return;
        }
        // Deleted or moved since: download again.
        JOBS.with(|jobs| jobs.borrow_mut().remove(&key));
    }
    let running = JOBS.with(|jobs| jobs.borrow_mut().remove(&key));
    if let Some(job) = running {
        if let Some(abort) = job.abort {
            abort.abort();
        }
        refresh(key.0, id);
        return;
    }
    let Some(dir) = default_dir() else {
        crate::menus::toast(tr!("There is no Downloads folder"));
        return;
    };
    start(&row, dir, None);
}

/// Starts downloading `row`'s file into `dir` (under `name` when given).
fn start(row: &MessageRow, dir: PathBuf, name: Option<String>) {
    let (peer, id) = (row.peer_id, row.id);
    let key = (peer, id);
    if let Some(job) = JOBS.with(|jobs| jobs.borrow_mut().remove(&key))
        && let Some(abort) = job.abort
    {
        abort.abort();
    }
    let total = row.file_size.max(0) as u64;
    let last = Arc::new(AtomicI32::new(-1));
    let Some(runtime) = RUNTIME.get() else { return };
    let handle = runtime.spawn(async move {
        let result = chats::save_file(tg(), peer, id, dir, name, move |bytes| {
            // Redraw at most once per percent (or per 256 KB when the size is unknown).
            let step = if total > 0 { (bytes.min(total) * 100 / total) as i32 } else { (bytes >> 18) as i32 };
            if last.swap(step, Ordering::Relaxed) == step {
                return;
            }
            let _ = slint::invoke_from_event_loop(move || {
                let known = JOBS.with(|jobs| {
                    let mut jobs = jobs.borrow_mut();
                    let Some(job) = jobs.get_mut(&key) else { return false };
                    job.bytes = bytes;
                    job.progress = if total > 0 { bytes.min(total) as f32 / total as f32 } else { 0.0 };
                    true
                });
                if known {
                    refresh(peer, id);
                }
            });
        })
        .await;
        let _ = slint::invoke_from_event_loop(move || done(key, result));
    });
    JOBS.with(|jobs| {
        jobs.borrow_mut().insert(key, Job { progress: 0.0, bytes: 0, path: None, abort: Some(handle.abort_handle()) });
    });
    refresh(peer, id);
}

fn done(key: (i64, i32), result: Result<String, oxigram_core::error::UiError>) {
    // Cancelled meanwhile: the job is gone.
    let present = JOBS.with(|jobs| jobs.borrow().contains_key(&key));
    if !present {
        return;
    }
    match result {
        Ok(path) => {
            JOBS.with(|jobs| {
                if let Some(job) = jobs.borrow_mut().get_mut(&key) {
                    job.path = Some(PathBuf::from(path));
                    job.abort = None;
                }
            });
        }
        Err(err) => {
            JOBS.with(|jobs| jobs.borrow_mut().remove(&key));
            crate::menus::toast(&format::error(&err));
        }
    }
    refresh(key.0, key.1);
}

/// Menu entries of a file message: "Показать в папке" once downloaded, and
/// "Сохранить как…".
pub fn menu_entries(m: &MessageRow) -> Vec<crate::MenuEntry> {
    use crate::context_menu::{SAVE, entry};
    let mut items = Vec::new();
    if !can_save(m) {
        return items;
    }
    if finished_path((m.peer_id, m.id)).is_some_and(|p| p.is_file()) {
        items.push(entry("file-reveal", &tr!("Show in Folder"), crate::context_menu::OPEN));
    }
    items.push(entry("file-save-as", &tr!("Save As…"), SAVE));
    items
}

/// Files, videos, audio and photos can be saved; self-destructing media is
/// refused by the core.
fn can_save(m: &MessageRow) -> bool {
    matches!(m.kind.as_str(), "document" | "video" | "gif" | "audio" | "voice" | "round" | "photo")
}

/// What the menu entries do.
pub fn menu_action(id: i32, action: &str) {
    let Some(row) = row_of(id) else { return };
    match action {
        "file-reveal" => {
            if let Some(path) = finished_path((row.peer_id, id)) {
                reveal(&path);
            }
        }
        "file-save-as" => save_as(row),
        _ => {}
    }
}

/// Asks where to save, then downloads there; the card shows the progress.
fn save_as(row: MessageRow) {
    let default = if row.file_name.trim().is_empty() {
        default_name(&row)
    } else {
        safe_name(&row.file_name)
    };
    let mut dialog = rfd::AsyncFileDialog::new().set_title(tr!("Save As")).set_file_name(&default);
    if let Some(dir) = dirs::download_dir() {
        dialog = dialog.set_directory(dir);
    }
    if let Some(ui) = ui()
        && let Some(with_parent) = ui.window().with_winit_window(|w| dialog.clone().set_parent(w))
    {
        dialog = with_parent;
    }
    let Some(runtime) = RUNTIME.get() else { return };
    runtime.spawn(async move {
        let Some(file) = dialog.save_file().await else { return };
        let path = file.path().to_path_buf();
        let _ = slint::invoke_from_event_loop(move || {
            let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return };
            start(&row, dir.to_path_buf(), Some(name.to_string_lossy().into_owned()));
        });
    });
}

/// A name for media without one of its own ("video_123.mp4").
fn default_name(row: &MessageRow) -> String {
    let (base, ext) = match row.kind.as_str() {
        "photo" => ("photo", "jpg"),
        "video" | "gif" | "round" => ("video", "mp4"),
        "voice" => ("voice", "ogg"),
        "audio" => ("audio", "mp3"),
        _ => ("file", "bin"),
    };
    format!("{base}_{}.{ext}", row.id)
}

/// Opens a file with the system's application for it.
fn open(path: &Path) {
    // ShellExecuteW "open": the application associated with the file type.
    #[cfg(windows)]
    let result: Result<(), ()> = {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::{PCWSTR, w};
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let code = unsafe { ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), None, None, SW_SHOWNORMAL) };
        // Values above 32 mean success.
        if code.0 as usize > 32 { Ok(()) } else { Err(()) }
    };
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(path).spawn().map(drop).map_err(drop);
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(path).spawn().map(drop).map_err(drop);
    if result.is_err() {
        crate::menus::toast(tr!("Could not open the file"));
    }
}

fn reveal(path: &Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(dir) = path.parent() {
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_is_short_and_upper() {
        assert_eq!(extension("report.pdf"), "PDF");
        assert_eq!(extension("archive.tar.gz"), "GZ");
        assert_eq!(extension("notes"), "");
        assert_eq!(extension(".bashrc"), "");
        assert_eq!(extension("x.verylong"), "");
    }

    #[test]
    fn safe_name_matches_the_core() {
        assert_eq!(safe_name("a:b?.txt"), "a_b_.txt");
        assert_eq!(safe_name("  x.txt. "), "x.txt");
    }
}
