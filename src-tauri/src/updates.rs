//! Live updates: one task reads Telegram's update stream, keeps the local
//! cache in step and forwards what the window needs to redraw.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use grammers_client::update::Update;
use grammers_client::{Client, InvocationError, tl};
use grammers_client::sender::UpdatesConfiguration;
use grammers_session::types::PeerId;
use grammers_session::updates::UpdatesLike;
use serde::Serialize;
use tauri::ipc::Channel;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};

use crate::chats::{self, PeerSummary};
use crate::store::Db;
use crate::store::chats::{self as cache, MessageRow, Presence};

/// Updates arriving within this window travel to the window as one batch,
/// so a burst costs one IPC hop and one redraw.
const BATCH_WINDOW: Duration = Duration::from_millis(30);
const MAX_BATCH: usize = 100;
const MAX_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UiEvent {
    /// A message was sent or received in `message.peer_id`.
    Message {
        message: MessageRow,
        /// Enough to show a chat that is not in the list yet.
        peer: PeerSummary,
    },
    Edited {
        message: MessageRow,
    },
    /// `peer_id` is only known for channels and supergroups.
    Deleted {
        peer_id: Option<i64>,
        ids: Vec<i32>,
    },
    Connection {
        online: bool,
    },
    /// The other side read own messages in `peer_id` up to `max_id`.
    ReadOutbox {
        peer_id: i64,
        max_id: i32,
    },
    /// The reactions under a message changed.
    Reactions {
        peer_id: i64,
        id: i32,
        reactions: Vec<cache::Reaction>,
    },
    /// Someone in `peer_id` is typing (or recording, uploading…); "cancel"
    /// when they stopped. Shown for a few seconds unless repeated.
    Typing {
        peer_id: i64,
        user_id: i64,
        action: &'static str,
    },
    /// The pinned messages of `peer_id` changed.
    Pinned {
        peer_id: i64,
    },
    /// Bytes of an outgoing file uploaded so far.
    Upload {
        token: String,
        bytes: u64,
    },
    /// A user's online status changed; `peer_id` is their private chat.
    Presence {
        peer_id: i64,
        presence: Presence,
    },
}

/// The window's end of the event stream. It is replaced whenever the window
/// (re)subscribes; with no window attached, events are simply not sent and the
/// cache alone keeps up.
#[derive(Default)]
pub struct UiSink {
    channel: Mutex<Option<Channel<Vec<UiEvent>>>>,
    /// The app, for system notifications; set once at start-up.
    app: std::sync::OnceLock<tauri::AppHandle>,
    /// The chat open in the window, or 0: its messages need no notification
    /// while the window has focus.
    active_chat: std::sync::atomic::AtomicI64,
}

impl UiSink {
    pub fn set_app(&self, app: tauri::AppHandle) {
        let _ = self.app.set(app);
    }

    pub fn set_active_chat(&self, peer_id: Option<i64>) {
        self.active_chat
            .store(peer_id.unwrap_or(0), std::sync::atomic::Ordering::Relaxed);
    }

    /// Shows a system notification for an incoming message unless the chat
    /// is muted or the user is looking at it right now. Works with the window
    /// closed to the tray too.
    fn notify(&self, db: &Db, message: &MessageRow, peer: &PeerSummary, sender: &str) {
        use tauri::Manager;
        use tauri_plugin_notification::NotificationExt;

        if message.outgoing {
            return;
        }
        let Some(app) = self.app.get() else { return };
        let looking = app.get_webview_window("main").is_some_and(|window| {
            window.is_focused().unwrap_or(false) && !window.is_minimized().unwrap_or(true)
        });
        let active = self.active_chat.load(std::sync::atomic::Ordering::Relaxed);
        if looking && active == message.peer_id {
            return;
        }
        let muted = db
            .lock()
            .ok()
            .and_then(|conn| cache::is_muted(&conn, message.peer_id))
            // A chat missing from the list: private chats ring, others stay quiet.
            .unwrap_or(message.peer_id < 0);
        if muted {
            return;
        }
        let title = if peer.kind == "saved" {
            "Избранное".to_owned()
        } else if peer.title.is_empty() {
            "Oxigram".to_owned()
        } else {
            peer.title.clone()
        };
        let text = notification_text(message);
        let body = if peer.kind == "group" && !sender.is_empty() {
            format!("{sender}: {text}")
        } else {
            text
        };
        let _ = app.notification().builder().title(title).body(body).show();
    }

    pub fn attach(&self, channel: Channel<Vec<UiEvent>>) {
        if let Ok(mut slot) = self.channel.lock() {
            *slot = Some(channel);
        }
    }

    /// Reports how much of an outgoing file is uploaded.
    pub fn upload_progress(&self, token: &str, bytes: u64) {
        self.send(vec![UiEvent::Upload {
            token: token.to_owned(),
            bytes,
        }]);
    }

    fn send(&self, events: Vec<UiEvent>) {
        if events.is_empty() {
            return;
        }
        let Ok(mut slot) = self.channel.lock() else {
            return;
        };
        // A failed send means the window is gone: stop trying until it returns.
        if slot.as_ref().is_some_and(|channel| channel.send(events).is_err()) {
            *slot = None;
        }
    }
}

pub async fn run(client: Client, receiver: mpsc::Receiver<UpdatesLike>, db: Db, ui: Arc<UiSink>) {
    // Missed messages are not replayed: every chat is refetched when opened.
    let configuration = UpdatesConfiguration { catch_up: false };
    let mut stream = match client.stream_updates(receiver, configuration).await {
        Ok(stream) => stream,
        Err(_) => {
            ui.send(vec![UiEvent::Connection { online: false }]);
            return;
        }
    };

    let mut online = true;
    let mut backoff = Duration::from_secs(1);
    loop {
        let first = match stream.next().await {
            Ok(update) => update,
            // The network task is gone (sign-out or shutdown): nothing to read.
            Err(InvocationError::Dropped(_)) => return,
            Err(_) => {
                if online {
                    online = false;
                    ui.send(vec![UiEvent::Connection { online: false }]);
                }
                sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };
        backoff = Duration::from_secs(1);

        let mut events = Vec::new();
        if !online {
            online = true;
            events.push(UiEvent::Connection { online: true });
        }
        events.extend(apply(&db, first));
        notify_new(&ui, &db, &events);
        // Gather whatever else arrives right away into the same batch.
        while events.len() < MAX_BATCH {
            match timeout(BATCH_WINDOW, stream.next()).await {
                Ok(Ok(update)) => {
                    let start = events.len();
                    events.extend(apply(&db, update));
                    notify_new(&ui, &db, &events[start..]);
                }
                _ => break,
            }
        }
        ui.send(events);
    }
}

fn notify_new(ui: &UiSink, db: &Db, events: &[UiEvent]) {
    for event in events {
        if let UiEvent::Message { message, peer } = event {
            ui.notify(db, message, peer, &message.sender_name);
        }
    }
}

/// The body of a notification: the text, or what kind of message it is.
fn notification_text(message: &MessageRow) -> String {
    const MAX_CHARS: usize = 160;
    let text: String = message.text.chars().take(MAX_CHARS).collect();
    if !text.trim().is_empty() {
        return text;
    }
    match message.kind.as_str() {
        "photo" => "Фотография",
        "video" => "Видео",
        "round" => "Видеосообщение",
        "voice" => "Голосовое сообщение",
        "audio" => "Аудио",
        "sticker" => "Стикер",
        "gif" => "GIF",
        "document" => "Файл",
        "contact" => "Контакт",
        "poll" => "Опрос",
        "location" => "Геопозиция",
        _ => "Новое сообщение",
    }
    .to_owned()
}

/// Applies one update to the cache and returns the event for the window.
fn apply(db: &Db, update: Update) -> Option<UiEvent> {
    match update {
        Update::NewMessage(message) => {
            let peer_id = chats::dialog_id(message.peer_id());
            let row = chats::message_row(&message, peer_id);
            let peer = chats::peer_summary(message.peer());
            store(db, |conn| cache::save_messages(conn, std::slice::from_ref(&row)));
            Some(UiEvent::Message { message: row, peer })
        }
        Update::MessageEdited(message) => {
            let peer_id = chats::dialog_id(message.peer_id());
            let row = chats::message_row(&message, peer_id);
            store(db, |conn| cache::save_messages(conn, std::slice::from_ref(&row)));
            Some(UiEvent::Edited { message: row })
        }
        Update::MessageDeleted(deletion) => {
            let peer_id = deletion
                .channel_id()
                .and_then(PeerId::channel)
                .map(chats::dialog_id);
            let ids = deletion.into_messages();
            store(db, |conn| cache::delete_messages(conn, peer_id, &ids));
            Some(UiEvent::Deleted { peer_id, ids })
        }
        Update::Raw(raw) => raw_event(db, raw.raw),
        _ => None,
    }
}

/// Updates grammers has no wrapper for yet.
fn raw_event(db: &Db, update: tl::enums::Update) -> Option<UiEvent> {
    use tl::enums::Update as U;
    match update {
        U::ReadHistoryOutbox(read) => {
            let peer_id = chats::dialog_id(PeerId::from(read.peer));
            read_outbox(db, peer_id, read.max_id)
        }
        U::ReadChannelOutbox(read) => {
            let peer_id = chats::dialog_id(PeerId::channel(read.channel_id)?);
            read_outbox(db, peer_id, read.max_id)
        }
        U::MessageReactions(update) => {
            // Threads of forums and Saved Messages tags are not shown.
            if update.top_msg_id.is_some() || update.saved_peer_id.is_some() {
                return None;
            }
            let peer_id = chats::dialog_id(PeerId::from(update.peer));
            let reactions = chats::reactions(Some(&update.reactions));
            store(db, |conn| cache::set_reactions(conn, peer_id, update.msg_id, &reactions));
            Some(UiEvent::Reactions {
                peer_id,
                id: update.msg_id,
                reactions,
            })
        }
        U::UserTyping(typing) if typing.top_msg_id.is_none() => {
            let peer_id = chats::dialog_id(PeerId::user(typing.user_id)?);
            Some(UiEvent::Typing {
                peer_id,
                user_id: peer_id,
                action: typing_action(&typing.action),
            })
        }
        U::ChatUserTyping(typing) => Some(UiEvent::Typing {
            peer_id: chats::dialog_id(PeerId::chat(typing.chat_id)?),
            user_id: chats::dialog_id(PeerId::from(typing.from_id)),
            action: typing_action(&typing.action),
        }),
        U::ChannelUserTyping(typing) if typing.top_msg_id.is_none() => Some(UiEvent::Typing {
            peer_id: chats::dialog_id(PeerId::channel(typing.channel_id)?),
            user_id: chats::dialog_id(PeerId::from(typing.from_id)),
            action: typing_action(&typing.action),
        }),
        U::PinnedMessages(pinned) => Some(UiEvent::Pinned {
            peer_id: chats::dialog_id(PeerId::from(pinned.peer)),
        }),
        U::PinnedChannelMessages(pinned) => Some(UiEvent::Pinned {
            peer_id: chats::dialog_id(PeerId::channel(pinned.channel_id)?),
        }),
        U::UserStatus(status) => Some(UiEvent::Presence {
            peer_id: chats::dialog_id(PeerId::user(status.user_id)?),
            presence: chats::presence(&status.status),
        }),
        _ => None,
    }
}

/// What the other side is doing, as the window words it.
fn typing_action(action: &tl::enums::SendMessageAction) -> &'static str {
    use tl::enums::SendMessageAction as A;
    match action {
        A::SendMessageTypingAction => "typing",
        A::SendMessageCancelAction => "cancel",
        A::SendMessageRecordAudioAction | A::SendMessageUploadAudioAction(_) => "voice",
        A::SendMessageRecordRoundAction | A::SendMessageUploadRoundAction(_) => "round",
        A::SendMessageRecordVideoAction | A::SendMessageUploadVideoAction(_) => "video",
        A::SendMessageUploadPhotoAction(_) => "photo",
        A::SendMessageUploadDocumentAction(_) => "file",
        A::SendMessageChooseStickerAction => "sticker",
        _ => "typing",
    }
}

fn read_outbox(db: &Db, peer_id: i64, max_id: i32) -> Option<UiEvent> {
    store(db, |conn| cache::set_read_outbox(conn, peer_id, max_id));
    Some(UiEvent::ReadOutbox { peer_id, max_id })
}

/// Cache writes are best effort here: a failure only means the chat is
/// refetched from Telegram the next time it is opened.
fn store(db: &Db, write: impl FnOnce(&mut rusqlite::Connection) -> rusqlite::Result<()>) {
    if let Ok(mut conn) = db.lock() {
        let _ = write(&mut conn);
    }
}
