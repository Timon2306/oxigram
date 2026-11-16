//! Selecting several messages to delete or forward them together, as in
//! Telegram: "Выбрать" in a message's menu starts it, clicks on bubbles add or
//! remove them, the bar over the chat header acts on the selection.

use slint::SharedString;

use crate::{App, AppWindow, with_app};

impl App {
    /// Every message of the bubbles in `ids` (an album is several messages).
    pub fn with_albums(&self, ids: &[i32]) -> Vec<i32> {
        let mut out: Vec<i32> = Vec::new();
        for id in ids {
            if let Some(index) = self.messages.iter().position(|m| m.id == *id) {
                out.extend(self.messages[self.album(index)].iter().map(|m| m.id));
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Whether the selection can be deleted here: private chats and saved
    /// messages always, a channel when the user can post, elsewhere own
    /// messages only.
    fn can_delete_selected(&self) -> bool {
        let kind = self.chat_kind();
        if kind == "user" || kind == "saved" {
            return true;
        }
        let can_post = self.chat.and_then(|peer| self.dialog(peer)).is_none_or(|d| d.can_post);
        if kind == "channel" {
            return can_post;
        }
        self.selected
            .iter()
            .all(|id| self.messages.iter().find(|m| m.id == *id).is_some_and(|m| m.outgoing))
    }

    fn show_selection(&self) {
        if let Some(ui) = self.ui.upgrade() {
            ui.set_select_mode(!self.selected.is_empty());
            ui.set_selected_count(self.selected.len() as i32);
            ui.set_select_can_delete(self.can_delete_selected());
        }
    }

    fn redraw(&self, id: i32) {
        if let Some(index) = self.messages.iter().position(|m| m.id == id) {
            self.refresh_row(index);
        }
    }

    /// Leaves selection mode (also when the chat changes).
    pub fn clear_selection(&mut self) {
        let old = std::mem::take(&mut self.selected);
        for id in old {
            self.redraw(id);
        }
        self.show_selection();
    }
}

pub fn wire(window: &AppWindow) {
    window.on_pick_message(toggle);
    window.on_select_cancel(|| {
        with_app(App::clear_selection);
    });
    window.on_select_delete(|| {
        let ids = with_app(|app| app.with_albums(&app.selected.clone())).unwrap_or_default();
        if !ids.is_empty() {
            crate::messages::ask_delete(ids, true);
        }
    });
    window.on_select_forward(|| {
        let ids = with_app(|app| app.with_albums(&app.selected.clone())).unwrap_or_default();
        if !ids.is_empty() {
            crate::messages::open_forward(ids);
            with_app(App::clear_selection);
        }
    });
}

/// "Выбрать" in the menu: selection mode with this message selected.
pub fn start(id: i32) {
    with_app(|app| {
        if !app.selected.contains(&id) {
            app.selected.push(id);
        }
        app.redraw(id);
        app.show_selection();
        if let Some(ui) = app.ui.upgrade() {
            // Nothing to type while selecting.
            ui.set_bar_mode(SharedString::new());
        }
    });
}

/// A click on a bubble while selecting: adds or removes it; the last one
/// removed ends selection mode.
fn toggle(id: i32) {
    with_app(|app| {
        if app.selected.is_empty() {
            return;
        }
        match app.selected.iter().position(|s| *s == id) {
            Some(at) => {
                app.selected.remove(at);
            }
            None => app.selected.push(id),
        }
        app.redraw(id);
        app.show_selection();
    });
}
