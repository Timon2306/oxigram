//! Search: inside the open chat (Ctrl+F) and across everything from the chat
//! list's search field. Both ask Telegram after a short pause, as the
//! WebView build does; picking a message opens its chat at that message.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use oxigram_core::actions::{self, FoundChat, FoundMessage};
use oxigram_core::chats;
use oxigram_core::store::chats::{DialogRow, MessageRow};
use slint::{Model, ModelRc, SharedString, VecModel};

use crate::{App, AppWindow, DialogItem, SearchHit, format, request_avatar, run, tg, with_app};

/// Searching in a chat waits this long after the last key.
const CHAT_PAUSE: Duration = Duration::from_millis(350);
/// Searching everywhere waits a little longer.
const GLOBAL_PAUSE: Duration = Duration::from_millis(450);
/// Messages around a jump target: this many newer ones are loaded with it.
const NEWER_AHEAD: i32 = 25;
const WINDOW: usize = 60;

/// Bumped by every keystroke; a reply for an older one is dropped.
static CHAT_SEQ: AtomicU64 = AtomicU64::new(0);
static GLOBAL_SEQ: AtomicU64 = AtomicU64::new(0);

/// Search results and what a jump changed, owned by `App`.
pub struct State {
    /// Hits in the open chat, newest first, and the one jumped to.
    results: Vec<MessageRow>,
    current: Option<usize>,
    hit_model: Rc<VecModel<SearchHit>>,
    /// The loaded messages are a window around a jump, not the chat's end.
    pub detached: bool,
    /// Chats met only in search results (not in the list), so their rows
    /// and header can still be drawn.
    strangers: HashMap<i64, DialogRow>,
    /// Peers of the rows in `found_chats` / `found_messages`.
    chat_peers: Vec<i64>,
    message_peers: Vec<(i64, i32)>,
    chat_model: Rc<VecModel<DialogItem>>,
    message_model: Rc<VecModel<DialogItem>>,
}

impl State {
    pub fn new(ui: &AppWindow) -> Self {
        let hit_model = Rc::new(VecModel::default());
        let chat_model = Rc::new(VecModel::default());
        let message_model = Rc::new(VecModel::default());
        ui.set_search_hits(ModelRc::from(hit_model.clone()));
        ui.set_found_chats(ModelRc::from(chat_model.clone()));
        ui.set_found_messages(ModelRc::from(message_model.clone()));
        State {
            results: Vec::new(),
            current: None,
            hit_model,
            detached: false,
            strangers: HashMap::new(),
            chat_peers: Vec::new(),
            message_peers: Vec::new(),
            chat_model,
            message_model,
        }
    }

    /// A chat that is not in the dialog list.
    pub fn stranger(&self, peer: i64) -> Option<&DialogRow> {
        self.strangers.get(&peer)
    }

    /// Another chat opens, or the chat closes: the bar and its hits go.
    pub fn reset(&mut self, ui: &AppWindow) {
        CHAT_SEQ.fetch_add(1, Ordering::SeqCst);
        self.results.clear();
        self.current = None;
        self.hit_model.clear();
        self.detached = false;
        ui.set_detached(false);
        ui.set_search_open(false);
        ui.set_search_count(SharedString::new());
        ui.invoke_clear_search_field();
        ui.set_highlight_id(-1);
        ui.set_jump_id(-1);
    }

    /// A downloaded photo reaches the search rows.
    pub fn avatar(&self, peer: i64, photo: &slint::Image) {
        for (peers, model) in [
            (self.chat_peers.clone(), &self.chat_model),
            (self.message_peers.iter().map(|(p, _)| *p).collect(), &self.message_model),
        ] {
            for (index, id) in peers.iter().enumerate() {
                if *id == peer
                    && let Some(mut item) = model.row_data(index)
                {
                    item.photo = photo.clone();
                    item.has_photo = true;
                    model.set_row_data(index, item);
                }
            }
        }
    }

    fn clear_found(&mut self) {
        self.chat_peers.clear();
        self.message_peers.clear();
        self.chat_model.clear();
        self.message_model.clear();
    }
}

pub fn wire(ui: &AppWindow) {
    ui.on_chat_search(chat_search);
    ui.on_search_pick(|index| pick(index.max(0) as usize));
    ui.on_search_step(step);
    ui.on_search_close(|| {
        with_app(|app| {
            CHAT_SEQ.fetch_add(1, Ordering::SeqCst);
            app.search.results.clear();
            app.search.current = None;
            app.search.hit_model.clear();
            if let Some(ui) = app.ui.upgrade() {
                ui.set_search_count(SharedString::new());
            }
        });
    });
    ui.on_jump_latest(|| {
        if let Some(Some(peer)) = with_app(|app| app.chat) {
            crate::open_chat(peer);
        }
    });
    ui.on_global_search(global_search);
    ui.on_open_found_chat(|index| {
        let peer = with_app(|app| app.search.chat_peers.get(index.max(0) as usize).copied()).flatten();
        if let Some(peer) = peer {
            crate::open_chat(peer);
        }
    });
    ui.on_open_found_message(|index| {
        let hit = with_app(|app| app.search.message_peers.get(index.max(0) as usize).copied()).flatten();
        if let Some((peer, id)) = hit {
            crate::open_chat_at(peer, Some(id));
        }
    });
    ui.on_want_found_avatar(|index, chat| {
        let index = index.max(0) as usize;
        let peer = with_app(|app| {
            if chat {
                app.search.chat_peers.get(index).copied()
            } else {
                app.search.message_peers.get(index).map(|(p, _)| *p)
            }
        })
        .flatten();
        if let Some(peer) = peer {
            request_avatar(peer);
        }
    });
}

// ---- Inside a chat

/// One line of a message for lists: the text on a single line, or what
/// stands in for its content.
fn snippet(m: &MessageRow) -> String {
    let flat = m.text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return format::kind_label(&m.kind).unwrap_or("Вложение").to_owned();
    }
    let shown = format::preview_text(&flat);
    shown.chars().take(200).collect()
}

fn hit_row(app: &App, peer: i64, m: &MessageRow) -> SearchHit {
    let who = if m.outgoing {
        "Вы".to_owned()
    } else if !m.sender_name.is_empty() {
        m.sender_name.clone()
    } else {
        app.dialog(peer).map(format::dialog_title).unwrap_or_default()
    };
    SearchHit { who: who.into(), text: snippet(m).into(), date: format::list_date(m.date).into() }
}

fn chat_search(text: SharedString, now: bool) {
    let seq = CHAT_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let query = text.trim().to_owned();
    let Some(Some((peer, generation))) = with_app(|app| {
        let peer = app.chat?;
        app.search.results.clear();
        app.search.current = None;
        app.search.hit_model.clear();
        if let Some(ui) = app.ui.upgrade() {
            ui.set_search_count(if query.is_empty() { "" } else { "…" }.into());
        }
        Some((peer, app.generation))
    }) else {
        return;
    };
    if query.is_empty() {
        return;
    }
    run(
        async move {
            if !now {
                tokio::time::sleep(CHAT_PAUSE).await;
            }
            if CHAT_SEQ.load(Ordering::SeqCst) != seq {
                return None;
            }
            Some(actions::search_chat(tg(), peer, &query, 50).await)
        },
        move |found| {
            let Some(found) = found else { return };
            with_app(|app| {
                if CHAT_SEQ.load(Ordering::SeqCst) != seq || app.generation != generation {
                    return;
                }
                let Some(ui) = app.ui.upgrade() else { return };
                match found {
                    Ok(rows) => {
                        let hits: Vec<SearchHit> = rows.iter().map(|m| hit_row(app, peer, m)).collect();
                        ui.set_search_count(
                            if rows.is_empty() { "Ничего не найдено".to_owned() } else { format!("Найдено: {}", rows.len()) }
                                .into(),
                        );
                        app.search.hit_model.set_vec(hits);
                        app.search.results = rows;
                    }
                    Err(err) => ui.set_search_count(format::error(&err).chars().take(40).collect::<String>().into()),
                }
            });
        },
    );
}

fn step(direction: i32) {
    let next = with_app(|app| {
        let n = app.search.results.len();
        if n == 0 {
            return None;
        }
        // Up goes to older hits, which sit later in the newest-first list.
        Some(match app.search.current {
            None => 0,
            Some(i) => (i as i32 + direction).clamp(0, n as i32 - 1) as usize,
        })
    })
    .flatten();
    if let Some(index) = next {
        pick(index);
    }
}

fn pick(index: usize) {
    let target = with_app(|app| {
        let id = app.search.results.get(index)?.id;
        app.search.current = Some(index);
        if let Some(ui) = app.ui.upgrade() {
            ui.set_search_count(format!("{} из {}", index + 1, app.search.results.len()).into());
            ui.set_search_list_open(false);
        }
        Some((app.chat?, id, app.generation))
    })
    .flatten();
    if let Some((peer, id, generation)) = target {
        goto_message(peer, id, generation);
    }
}

/// Brings a message of the open chat into view: loaded ones are scrolled to,
/// others come with the history around them.
fn goto_message(peer: i64, id: i32, generation: u64) {
    let loaded = with_app(|app| {
        if app.chat != Some(peer) || app.generation != generation {
            return true;
        }
        match app.messages.iter().position(|m| m.id == id) {
            Some(index) => {
                // An album's bubble sits on its first row.
                let head = app.album(index).start;
                if let Some(ui) = app.ui.upgrade() {
                    ui.invoke_jump_to(head as i32, app.messages[head].id);
                }
                true
            }
            None => {
                app.history_loading = true;
                if let Some(ui) = app.ui.upgrade() {
                    ui.set_history_loading(true);
                }
                false
            }
        }
    })
    .unwrap_or(true);
    if !loaded {
        load_window(peer, id, generation);
    }
}

/// Loads the page that holds `target` with some newer messages after it, and
/// shows it there. `history_loading` is already set.
pub fn load_window(peer: i64, target: i32, generation: u64) {
    run(
        async move { chats::history(tg(), peer, Some(target.saturating_add(NEWER_AHEAD)), WINDOW).await },
        move |page| {
            with_app(|app| {
                if app.generation != generation {
                    return;
                }
                app.history_loading = false;
                if let Some(ui) = app.ui.upgrade() {
                    ui.set_history_loading(false);
                }
                match page {
                    Ok(mut rows) => {
                        app.history_done = rows.len() < WINDOW;
                        rows.reverse();
                        app.messages = rows;
                        let items = (0..app.messages.len()).map(|i| app.item(i)).collect::<Vec<_>>();
                        app.message_model.set_vec(items);
                        app.search.detached = true;
                        if let Some(ui) = app.ui.upgrade() {
                            ui.set_detached(true);
                        }
                        if let Some(index) = app.messages.iter().position(|m| m.id == target) {
                            let head = app.album(index).start;
                            if let Some(ui) = app.ui.upgrade() {
                                ui.invoke_jump_to(head as i32, app.messages[head].id);
                            }
                        }
                    }
                    Err(err) => app.chat_error(&err),
                }
            });
        },
    );
}

// ---- The chat list's search field

/// "@name", "t.me/name" or "https://t.me/name": the public username in it.
pub fn username_of(query: &str) -> Option<String> {
    let q = query.trim();
    let rest = match q.strip_prefix('@') {
        Some(rest) => rest,
        None => {
            let link = q.strip_prefix("https://").or_else(|| q.strip_prefix("http://")).unwrap_or(q);
            link.strip_prefix("t.me/").or_else(|| link.strip_prefix("telegram.me/"))?
        }
    };
    let name = rest.split(['/', '?', '#']).next()?;
    let valid = (3..=32).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then(|| name.to_owned())
}

fn global_search(text: SharedString) {
    let seq = GLOBAL_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let query = text.trim().to_owned();
    let username = username_of(&query);
    let short = query.chars().count() < 2 && username.is_none();
    with_app(|app| {
        app.search.clear_found();
        if let Some(ui) = app.ui.upgrade() {
            let note = if query.is_empty() {
                ""
            } else if short {
                if app.shown.is_empty() { "Ничего не найдено" } else { "" }
            } else {
                "Поиск сообщений…"
            };
            ui.set_found_note(note.into());
        }
    });
    if short {
        return;
    }
    run(
        async move {
            tokio::time::sleep(GLOBAL_PAUSE).await;
            if GLOBAL_SEQ.load(Ordering::SeqCst) != seq {
                return None;
            }
            let messages = actions::search_global(tg(), &query, 30).await.unwrap_or_default();
            let chat = match username {
                Some(name) if GLOBAL_SEQ.load(Ordering::SeqCst) == seq => actions::resolve_username(tg(), &name).await.ok(),
                _ => None,
            };
            Some((messages, chat))
        },
        move |found| {
            let Some((messages, chat)) = found else { return };
            with_app(|app| {
                if GLOBAL_SEQ.load(Ordering::SeqCst) != seq {
                    return;
                }
                show_found(app, messages, chat);
            });
        },
    );
}

/// Keeps a chat found by @username (a mention, a t.me link) so it opens like
/// a chat of the list.
pub(crate) fn remember_stranger(app: &mut App, chat: &FoundChat) {
    if app.dialog(chat.id).is_none() {
        let row = stranger_row(chat.id, &chat.kind, &chat.title, chat.photo_id.clone());
        app.search.strangers.insert(chat.id, row);
    }
}

fn stranger_row(id: i64, kind: &str, title: &str, photo_id: Option<String>) -> DialogRow {
    DialogRow {
        id,
        kind: kind.to_owned(),
        title: title.to_owned(),
        last_text: String::new(),
        last_date: 0,
        last_outgoing: false,
        last_id: 0,
        unread: 0,
        pinned: false,
        archived: false,
        photo_id,
        read_outbox: 0,
        unread_reactions: 0,
        muted: false,
        contact: false,
        bot: false,
        presence: None,
        can_post: kind != "channel",
    }
}

fn found_item(app: &App, peer: i64, kind: &str, title: &str, preview: String, date: i64) -> DialogItem {
    let saved = kind == "saved";
    let title = if saved { "Избранное" } else if title.is_empty() { "Без названия" } else { title };
    let photo = app.avatars.get(&peer);
    DialogItem {
        initial: format::initial(title).into(),
        title: title.into(),
        color: format::avatar_color(peer),
        photo: photo.cloned().unwrap_or_default(),
        has_photo: photo.is_some(),
        preview: preview.into(),
        time: if date > 0 { format::list_date(date).into() } else { SharedString::new() },
        saved,
        kind: kind.into(),
        ..Default::default()
    }
}

fn show_found(app: &mut App, messages: Vec<FoundMessage>, chat: Option<FoundChat>) {
    app.search.clear_found();
    // A chat by its @username, unless the list already shows it.
    if let Some(chat) = chat
        && !app.shown.contains(&chat.id)
    {
        if app.dialog(chat.id).is_none() {
            let row = stranger_row(chat.id, &chat.kind, &chat.title, chat.photo_id.clone());
            app.search.strangers.insert(chat.id, row);
        }
        let item = found_item(app, chat.id, &chat.kind, &chat.title, String::new(), 0);
        app.search.chat_peers.push(chat.id);
        app.search.chat_model.push(item);
    }
    let mut items = Vec::with_capacity(messages.len());
    for hit in &messages {
        let peer = hit.message.peer_id;
        if app.dialog(peer).is_none() {
            let row = stranger_row(peer, &hit.peer.kind, &hit.peer.title, hit.peer.photo_id.clone());
            app.search.strangers.insert(peer, row);
        }
        items.push(found_item(app, peer, &hit.peer.kind, &hit.peer.title, snippet(&hit.message), hit.message.date));
        app.search.message_peers.push((peer, hit.message.id));
    }
    let nothing = items.is_empty() && app.search.chat_peers.is_empty() && app.shown.is_empty();
    app.search.message_model.set_vec(items);
    if let Some(ui) = app.ui.upgrade() {
        ui.set_found_note(if nothing { "Ничего не найдено" } else { "" }.into());
    }
}

#[cfg(test)]
mod tests {
    use super::username_of;

    #[test]
    fn usernames() {
        assert_eq!(username_of("@durov").as_deref(), Some("durov"));
        assert_eq!(username_of(" t.me/some_chan/12 ").as_deref(), Some("some_chan"));
        assert_eq!(username_of("https://t.me/abc?start=1").as_deref(), Some("abc"));
        assert_eq!(username_of("привет"), None);
        assert_eq!(username_of("@ab"), None);
        assert_eq!(username_of("t.me/+inviteHash"), None);
    }
}
