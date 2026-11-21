//! The quote above a reply: who wrote the message it answers and a line of
//! it. The answered message is taken from the loaded history when it is
//! there; older ones are fetched by ID in one request per batch and kept for
//! the open chat.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use oxigram_core::chats;
use oxigram_core::store::chats::MessageRow;

use crate::{App, format, run, tg, with_app};

/// Quotes of the open chat's answered messages that are not loaded.
#[derive(Default)]
pub struct Replies {
    /// Message ID → (who, line); a deleted message gets an empty "who".
    known: HashMap<i32, (String, String)>,
    /// Asked for already (or in flight).
    asked: RefCell<HashSet<i32>>,
    /// Waiting for the next batch.
    wanted: RefCell<Vec<i32>>,
}

impl Replies {
    pub fn clear(&mut self) {
        self.known.clear();
        self.asked.borrow_mut().clear();
        self.wanted.borrow_mut().clear();
    }
}

/// Who wrote `row`, as the quote names them.
fn who(app: &App, row: &MessageRow) -> String {
    if row.outgoing {
        return "You".to_owned();
    }
    if !row.sender_name.is_empty() {
        return row.sender_name.clone();
    }
    // Channel posts have no sender: the channel's name.
    app.chat.and_then(|peer| app.dialog(peer)).map(format::dialog_title).unwrap_or_default()
}

/// One line of `row`: its text, or what it contains ("Фото", "Файл"…).
fn line(row: &MessageRow) -> String {
    let text = row.text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    if !text.is_empty() {
        return text.to_owned();
    }
    if row.kind == "document" && !row.file_name.is_empty() {
        return row.file_name.clone();
    }
    format::kind_label(&row.kind).unwrap_or("Message").to_owned()
}

/// The quote of a reply: (who, line), or `None` for a message that answers
/// nothing. A quote still loading reads "…" and is fetched soon.
pub fn quote(app: &App, row: &MessageRow) -> Option<(String, String)> {
    let target = row.reply_to_id?;
    if let Some(answered) = app.messages.iter().find(|m| m.id == target) {
        return Some((who(app, answered), line(answered)));
    }
    if let Some((who, line)) = app.replies.known.get(&target) {
        return Some((who.clone(), line.clone()));
    }
    if app.replies.asked.borrow_mut().insert(target) {
        let first = {
            let mut wanted = app.replies.wanted.borrow_mut();
            wanted.push(target);
            wanted.len() == 1
        };
        // Rows are being built now: fetch once they are all known.
        if first {
            slint::Timer::single_shot(Duration::ZERO, fetch);
        }
    }
    Some((String::new(), "…".to_owned()))
}

/// Fetches the answered messages that are not loaded, then redraws the
/// replies that quote them.
fn fetch() {
    let Some(Some((peer, generation, ids))) = with_app(|app| {
        let ids: Vec<i32> = std::mem::take(&mut *app.replies.wanted.borrow_mut());
        Some((app.chat?, app.generation, ids))
    }) else {
        return;
    };
    if ids.is_empty() {
        return;
    }
    run(async move { chats::messages_by_id(tg(), peer, &ids).await }, move |found| {
        with_app(|app| {
            if app.generation != generation {
                return;
            }
            let Ok(found) = found else { return };
            let mut done = HashSet::new();
            for (id, row) in found {
                let quote = match row {
                    Some(row) => (who(app, &row), line(&row)),
                    None => (String::new(), "Message deleted".to_owned()),
                };
                app.replies.known.insert(id, quote);
                done.insert(id);
            }
            let rows: Vec<usize> = app
                .messages
                .iter()
                .enumerate()
                .filter(|(_, m)| m.reply_to_id.is_some_and(|r| done.contains(&r)))
                .map(|(i, _)| i)
                .collect();
            for index in rows {
                app.refresh_row(index);
            }
        });
    });
}

/// A click on the quote: the answered message.
pub fn open(id: i32) {
    let target = with_app(|app| {
        let row = app.messages.iter().find(|m| m.id == id)?;
        Some((app.chat?, row.reply_to_id?, app.generation))
    })
    .flatten();
    if let Some((peer, target, generation)) = target {
        crate::search::goto_message(peer, target, generation);
    }
}
