//! Chat folders (Telegram's dialog filters) as tabs above the chat list.
//! Same rules as `inFolder` in `src/lib/ipc.ts` of the WebView build; the
//! chosen tab is remembered in `native-folder` next to the settings.

use std::cell::RefCell;

use oxigram_core::chats::{self, Folder};
use oxigram_core::store::chats::DialogRow;
use slint::{ModelRc, VecModel};

use crate::{AppWindow, FolderTab, run, tg, ui, with_app};

const FILE: &str = "native-folder";

struct State {
    list: Vec<Folder>,
    /// 0 is "Все чаты".
    active: i32,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { list: Vec::new(), active: load_active() });
}

fn load_active() -> i32 {
    crate::settings::dir()
        .and_then(|dir| std::fs::read_to_string(dir.join(FILE)).ok())
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

fn save_active(id: i32) {
    if let Some(dir) = crate::settings::dir() {
        let _ = std::fs::write(dir.join(FILE), id.to_string());
    }
}

/// Whether a chat belongs to a folder.
pub fn in_folder(dialog: &DialogRow, folder: &Folder) -> bool {
    if folder.exclude.contains(&dialog.id) {
        return false;
    }
    if folder.include.contains(&dialog.id) {
        return true;
    }
    if folder.exclude_archived && dialog.archived {
        return false;
    }
    if folder.exclude_muted && dialog.muted {
        return false;
    }
    if folder.exclude_read && dialog.unread == 0 {
        return false;
    }
    match dialog.kind.as_str() {
        "user" if dialog.bot => folder.bots,
        "user" if dialog.contact => folder.contacts,
        "user" => folder.non_contacts,
        "group" => folder.groups,
        "channel" => folder.channels,
        _ => false,
    }
}

/// The folder whose chats the list shows now; none for "Все чаты" (or when
/// the remembered folder no longer exists).
fn with_active<R>(f: impl FnOnce(Option<&Folder>) -> R) -> R {
    STATE.with(|state| {
        let state = state.borrow();
        f(state.list.iter().find(|folder| folder.id == state.active))
    })
}

/// Whether the main list (not the archive, not a search) shows this chat.
pub fn keep(dialog: &DialogRow) -> bool {
    with_active(|folder| match folder {
        Some(folder) => in_folder(dialog, folder),
        None => !dialog.archived,
    })
}

/// "Все чаты" is the one showing, so the archive row belongs to the list.
pub fn is_all() -> bool {
    with_active(|folder| folder.is_none())
}

/// Redraws the tabs and their unread counters (chats with unread messages,
/// not muted ones, as in the WebView build).
pub fn refresh_tabs(ui: &AppWindow, dialogs: &[DialogRow]) {
    let tabs: Vec<FolderTab> = STATE.with(|state| {
        let state = state.borrow();
        if state.list.is_empty() {
            return Vec::new();
        }
        let current = state.list.iter().any(|f| f.id == state.active);
        let mut tabs = vec![FolderTab { title: "Все чаты".into(), unread: 0, on: !current }];
        for folder in &state.list {
            let unread = dialogs
                .iter()
                .filter(|d| d.unread > 0 && !d.muted && in_folder(d, folder))
                .count();
            tabs.push(FolderTab {
                title: folder.title.as_str().into(),
                unread: unread as i32,
                on: current && folder.id == state.active,
            });
        }
        tabs
    });
    ui.set_folder_tabs(ModelRc::new(VecModel::from(tabs)));
    ui.set_folder_all(is_all());
}

/// Fetches the folders once the account is ready.
pub fn load() {
    let me = with_app(|app| app.me_id).unwrap_or(0);
    run(async move { chats::folders(tg(), me).await }, |found| {
        let Ok(list) = found else { return };
        STATE.with(|state| state.borrow_mut().list = list);
        with_app(|app| app.refresh_dialogs());
    });
}

/// Forgets everything (sign-out).
pub fn clear() {
    STATE.with(|state| state.borrow_mut().list.clear());
}

pub fn wire(window: &AppWindow) {
    window.on_pick_folder(|index| {
        let changed = STATE.with(|state| {
            let mut state = state.borrow_mut();
            let id = if index <= 0 { 0 } else { state.list.get(index as usize - 1).map_or(0, |f| f.id) };
            let changed = state.active != id;
            state.active = id;
            if changed {
                save_active(id);
            }
            changed
        });
        if changed {
            with_app(|app| app.refresh_dialogs());
            if let Some(ui) = ui() {
                ui.invoke_scroll_dialogs_top();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> Folder {
        Folder {
            id: 2,
            title: "Работа".into(),
            emoticon: None,
            contacts: true,
            non_contacts: false,
            groups: true,
            channels: false,
            bots: false,
            exclude_muted: true,
            exclude_read: false,
            exclude_archived: true,
            include: vec![7],
            exclude: vec![9],
        }
    }

    fn dialog(id: i64, kind: &str) -> DialogRow {
        DialogRow {
            id,
            kind: kind.into(),
            title: String::new(),
            last_text: String::new(),
            last_date: 0,
            last_outgoing: false,
            last_id: 0,
            unread: 0,
            pinned: false,
            archived: false,
            photo_id: None,
            read_outbox: 0,
            unread_reactions: 0,
            muted: false,
            contact: false,
            bot: false,
            presence: None,
            can_post: true,
        }
    }

    #[test]
    fn rules_follow_the_webview_build() {
        let f = folder();
        assert!(in_folder(&dialog(1, "group"), &f));
        assert!(!in_folder(&dialog(1, "channel"), &f));
        assert!(!in_folder(&dialog(1, "user"), &f), "not a contact");
        assert!(in_folder(&DialogRow { contact: true, ..dialog(1, "user") }, &f));
        // Listed by hand, whatever the kind or flags
        assert!(in_folder(&DialogRow { muted: true, ..dialog(7, "channel") }, &f));
        // Excluded wins over the kind
        assert!(!in_folder(&dialog(9, "group"), &f));
        assert!(!in_folder(&DialogRow { muted: true, ..dialog(1, "group") }, &f));
        assert!(!in_folder(&DialogRow { archived: true, ..dialog(1, "group") }, &f));
    }
}
