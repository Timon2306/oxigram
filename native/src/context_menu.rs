//! Right-click menus, drawn by the window itself rather than by the platform.

use slint::{ModelRc, VecModel};

use crate::{AppWindow, MenuEntry, menus, messages, with_app};

// 24x24 stroke icons, the same paths as the web client's context menu.
pub(crate) const OPEN: &str = "M5 12h14M13 6l6 6-6 6";
const PIN: &str = "M9 4h6l-1 6 3 3v2H7v-2l3-3-1-6zM12 15v5";
const UNPIN: &str = "M9 4h6l-1 6 3 3v2H7v-2l3-3-1-6zM12 15v5M4 4l16 16";
const READ: &str = "M3 12.5l4 4 7-9M11 15l2 2 8-10";
const UNREAD: &str = "M12 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10z";
const ARCHIVE: &str = "M4 5h16v4H4zM5 9v10h14V9M10 13h4";
const UNARCHIVE: &str = "M4 5h16v4H4zM5 9v10h14V9M12 17v-5M9.5 14.5 12 12l2.5 2.5";
const CLEAR: &str = "M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13M10 11v6M14 11v6";
const DELETE: &str = "M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13";
const LEAVE: &str = "M10 4H5v16h5M14 8l4 4-4 4M18 12H9";
const REPLY: &str = "M10 7 4 12l6 5v-3c5 0 8 1 10 5 0-6-3-10-10-10V7z";
/// A text cursor between selection handles.
const SELECT: &str = "M9 4h6M9 20h6M12 4v16";
pub(crate) const COPY: &str = "M8 8h11v12H8zM5 16V4h11";
const EDIT: &str = "M4 20h4L19 9l-4-4L4 16v4zM13 7l4 4";
pub(crate) const FORWARD: &str = "M14 7l6 5-6 5v-3c-5 0-8 1-10 5 0-6 3-10 10-10V7z";
/// A ticked circle.
const CHOOSE: &str = "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM8 12.5l3 3 5-6";
pub(crate) const SAVE: &str = "M12 4v11M7 10l5 5 5-5M5 19h14";
const MUTE: &str = "M11 5 6 9H3v6h3l5 4V5zM16 9l5 6M21 9l-5 6";
const UNMUTE: &str = "M11 5 6 9H3v6h3l5 4V5zM15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13";

/// What the open menu acts on.
#[derive(Clone, Copy)]
pub(crate) enum MenuTarget {
    /// A chat in the list, by peer ID.
    Dialog(i64),
    /// A message of the open chat.
    Message(i32),
    /// The picture open in the viewer, by message ID.
    Viewer(i32),
}

pub(crate) fn entry(id: &str, label: &str, icon: &str) -> MenuEntry {
    MenuEntry { id: id.into(), label: label.into(), icon: icon.into(), danger: false }
}

fn danger(id: &str, label: &str, icon: &str) -> MenuEntry {
    MenuEntry { danger: true, ..entry(id, label, icon) }
}

fn separator() -> MenuEntry {
    entry("-", "", "")
}

pub(crate) fn show(items: Vec<MenuEntry>, target: MenuTarget, x: f32, y: f32) {
    with_app(|app| {
        app.menu_target = Some(target);
        if let Some(ui) = app.ui.upgrade() {
            ui.set_menu_items(ModelRc::new(VecModel::from(items)));
            // The quick reactions row belongs to message menus only.
            ui.set_menu_reactions(ModelRc::default());
            ui.set_menu_x(x);
            ui.set_menu_y(y);
            ui.set_menu_open(true);
        }
    });
}

pub fn wire(ui: &AppWindow) {
    ui.on_dialog_menu(|index, x, y| {
        let found = with_app(|app| {
            let peer = app.shown.get(index as usize).copied()?;
            app.dialog(peer).cloned()
        })
        .flatten();
        let Some(d) = found else { return };
        let mut items = vec![
            entry("open", "Открыть", OPEN),
            if d.pinned { entry("pin", "Открепить", UNPIN) } else { entry("pin", "Закрепить", PIN) },
            if d.unread > 0 {
                entry("read", "Отметить как прочитанное", READ)
            } else {
                entry("unread", "Отметить как непрочитанное", UNREAD)
            },
            if d.archived { entry("archive", "Из архива", UNARCHIVE) } else { entry("archive", "В архив", ARCHIVE) },
        ];
        if d.kind != "saved" {
            items.push(if d.muted {
                entry("mute", "Включить уведомления", UNMUTE)
            } else {
                entry("mute", "Выключить уведомления", MUTE)
            });
        }
        items.push(separator());
        if d.kind != "channel" {
            items.push(danger("clear", "Очистить историю", CLEAR));
        }
        items.push(match d.kind.as_str() {
            "group" => danger("delete", "Покинуть группу", LEAVE),
            "channel" => danger("delete", "Покинуть канал", LEAVE),
            _ => danger("delete", "Удалить чат", DELETE),
        });
        show(items, MenuTarget::Dialog(d.id), x, y);
    });

    ui.on_message_menu(|id, x, y| {
        let found = with_app(|app| {
            let m = app.messages.iter().find(|m| m.id == id).cloned()?;
            let can_post = app.chat.and_then(|peer| app.dialog(peer)).is_none_or(|d| d.can_post);
            Some((m, can_post))
        })
        .flatten();
        let Some((m, can_post)) = found else { return };
        // A channel read by a subscriber: only reading actions make sense.
        let mut items = Vec::new();
        if can_post {
            items.push(entry("reply", "Ответить", REPLY));
        }
        if m.kind == "photo" && m.playable {
            items.push(entry("copy-image", "Копировать картинку", COPY));
        }
        if !m.text.is_empty() {
            items.push(entry("copy", "Копировать текст", COPY));
            items.push(entry("select", "Выделить текст", SELECT));
        }
        if can_post && m.outgoing && m.kind == "text" {
            items.push(entry("edit", "Изменить", EDIT));
        }
        if can_post {
            items.push(entry("pin", "Закрепить", PIN));
        }
        items.extend(crate::files::menu_entries(&m));
        items.push(entry("forward", "Переслать", FORWARD));
        items.push(entry("choose", "Выбрать", CHOOSE));
        if can_post {
            items.push(separator());
            items.push(danger("delete", "Удалить", DELETE));
        }
        show(items, MenuTarget::Message(id), x, y);
        if let Some(ui) = crate::ui() {
            crate::reactions::fill_menu(&ui, id);
        }
    });

    ui.on_menu_pick(|id| {
        let target = with_app(|app| app.menu_target.take()).flatten();
        match target {
            Some(MenuTarget::Dialog(peer)) => menus::dialog_action(peer, id.as_str()),
            Some(MenuTarget::Message(message)) if id == "copy-image" => crate::pictures::copy_message_image(message),
            Some(MenuTarget::Message(message)) if id == "choose" => crate::select::start(message),
            Some(MenuTarget::Message(message)) if id.starts_with("file-") => crate::files::menu_action(message, id.as_str()),
            Some(MenuTarget::Message(message)) => messages::message_action(message, id),
            Some(MenuTarget::Viewer(message)) => crate::pictures::viewer_action(message, id.as_str()),
            None => {}
        }
    });
}
