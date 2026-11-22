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
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};

use crate::chats::{self, PeerSummary};
use crate::store::Db;
use crate::tr;
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
    /// The chat was read (on another device too): `unread` messages are left.
    ReadInbox {
        peer_id: i64,
        unread: i32,
    },
    /// "Отметить как (не)прочитанное", on any device.
    UnreadMark {
        peer_id: i64,
        unread: bool,
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

/// Where events go: the window's queue.
/// `send` returns `false` once the window is gone.
pub trait EventSink: Send + Sync {
    fn send(&self, events: Vec<UiEvent>) -> bool;
}

/// System notifications and whether the user is looking at the window.
pub trait Notifier: Send + Sync {
    /// The main window is focused and not minimized.
    fn looking(&self) -> bool;
    /// `avatar`: the chat's peer ID and profile photo ID, when it has one.
    fn show(&self, title: String, body: String, avatar: Option<(i64, String)>);
}

/// The window's end of the event stream. It is replaced whenever the window
/// (re)subscribes; with no window attached, events are simply not sent and the
/// cache alone keeps up.
#[derive(Default)]
pub struct UiSink {
    channel: Mutex<Option<Box<dyn EventSink>>>,
    /// For system notifications; set once at start-up.
    notifier: std::sync::OnceLock<Box<dyn Notifier>>,
    /// The chat open in the window, or 0: its messages need no notification
    /// while the window has focus.
    active_chat: std::sync::atomic::AtomicI64,
}

impl UiSink {
    pub fn set_notifier(&self, notifier: Box<dyn Notifier>) {
        let _ = self.notifier.set(notifier);
    }

    pub fn set_active_chat(&self, peer_id: Option<i64>) {
        self.active_chat
            .store(peer_id.unwrap_or(0), std::sync::atomic::Ordering::Relaxed);
    }

    /// Shows a system notification for an incoming message unless the chat
    /// is muted or the user is looking at it right now. Works with the window
    /// closed to the tray too.
    fn notify(&self, db: &Db, message: &MessageRow, peer: &PeerSummary, sender: &str) {
        if message.outgoing {
            return;
        }
        let Some(notifier) = self.notifier.get() else { return };
        let active = self.active_chat.load(std::sync::atomic::Ordering::Relaxed);
        if active == message.peer_id && notifier.looking() {
            return;
        }
        let (muted, listed_title, listed_photo) = match db.lock() {
            Ok(conn) => (
                cache::is_muted(&conn, message.peer_id),
                cache::dialog_title(&conn, message.peer_id),
                cache::dialog_photo(&conn, message.peer_id),
            ),
            Err(_) => (None, None, None),
        };
        // A chat missing from the list: private chats ring, others stay quiet.
        if muted.unwrap_or(message.peer_id < 0) {
            return;
        }
        // Updates do not always carry the chat's details; the list has its
        // name, and in a private chat the sender is the chat.
        let title = if peer.kind == "saved" {
            tr!("Saved Messages")
        } else if !peer.title.is_empty() {
            peer.title.clone()
        } else if let Some(title) = listed_title {
            title
        } else if !sender.is_empty() {
            sender.to_owned()
        } else {
            tr!("New message")
        };
        let text = notification_text(message);
        let in_group = peer.kind == "group" || (peer.kind.is_empty() && message.peer_id < 0);
        let body = if in_group && !sender.is_empty() && sender != title {
            format!("{sender}: {text}")
        } else {
            text
        };
        // The chat's picture (a group's own, as in Telegram Desktop).
        let photo = peer.photo_id.clone().or(listed_photo);
        notifier.show(title, body, photo.map(|photo| (message.peer_id, photo)));
    }

    /// Tells the user about an incoming call. Oxigram cannot carry the audio,
    /// so the call is answered — or not — in Telegram's own app.
    pub fn notify_call(&self, db: &Db, caller_id: i64) {
        let Some(notifier) = self.notifier.get() else { return };
        let peer_id = PeerId::user(caller_id).map(chats::dialog_id);
        let name = match (peer_id, db.lock()) {
            (Some(peer_id), Ok(conn)) => cache::dialog_title(&conn, peer_id).unwrap_or_default(),
            _ => String::new(),
        };
        let title = if name.is_empty() { tr!("Incoming call") } else { name };
        notifier.show(title, tr!("Answer it in the Telegram app: calls are not supported yet"), None);
    }

    pub fn attach(&self, sink: Box<dyn EventSink>) {
        if let Ok(mut slot) = self.channel.lock() {
            *slot = Some(sink);
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
        if slot.as_ref().is_some_and(|sink| !sink.send(events)) {
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
        events.extend(apply(&db, &ui, first));
        notify_new(&ui, &db, &events);
        // Gather whatever else arrives right away into the same batch.
        while events.len() < MAX_BATCH {
            match timeout(BATCH_WINDOW, stream.next()).await {
                Ok(Ok(update)) => {
                    let start = events.len();
                    events.extend(apply(&db, &ui, update));
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
        "photo" => tr!("Photo"),
        "video" => tr!("Video"),
        "round" => tr!("Video message"),
        "voice" => tr!("Voice message"),
        "audio" => tr!("Audio"),
        "sticker" => tr!("Sticker"),
        "gif" => "GIF".to_owned(),
        "document" => tr!("File"),
        "contact" => tr!("Contact"),
        "poll" => tr!("Poll"),
        "location" => tr!("Location"),
        _ => tr!("New message"),
    }
}

/// Applies one update to the cache and returns the event for the window.
fn apply(db: &Db, ui: &UiSink, update: Update) -> Option<UiEvent> {
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
        Update::Raw(raw) => raw_event(db, ui, raw.raw),
        _ => None,
    }
}

/// Updates grammers has no wrapper for yet.
fn raw_event(db: &Db, ui: &UiSink, update: tl::enums::Update) -> Option<UiEvent> {
    use tl::enums::Update as U;
    match update {
        // A call to this account: Oxigram has no audio stack, so it only says
        // who is calling; the call itself is answered in Telegram's app.
        U::PhoneCall(call) => {
            if let tl::enums::PhoneCall::Requested(request) = call.phone_call {
                ui.notify_call(db, request.participant_id);
            }
            None
        }
        U::ReadHistoryOutbox(read) => {
            let peer_id = chats::dialog_id(PeerId::from(read.peer));
            read_outbox(db, peer_id, read.max_id)
        }
        U::ReadChannelOutbox(read) => {
            let peer_id = chats::dialog_id(PeerId::channel(read.channel_id)?);
            read_outbox(db, peer_id, read.max_id)
        }
        // Threads of forums keep their own counters.
        U::ReadHistoryInbox(read) if read.top_msg_id.is_none() => {
            let peer_id = chats::dialog_id(PeerId::from(read.peer));
            read_inbox(db, peer_id, read.still_unread_count)
        }
        U::ReadChannelInbox(read) => {
            let peer_id = chats::dialog_id(PeerId::channel(read.channel_id)?);
            read_inbox(db, peer_id, read.still_unread_count)
        }
        U::DialogUnreadMark(mark) if mark.saved_peer_id.is_none() => {
            let tl::enums::DialogPeer::Peer(dialog) = mark.peer else { return None };
            let peer_id = chats::dialog_id(PeerId::from(dialog.peer));
            store(db, |conn| cache::set_unread_mark(conn, peer_id, mark.unread));
            Some(UiEvent::UnreadMark { peer_id, unread: mark.unread })
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

fn read_inbox(db: &Db, peer_id: i64, unread: i32) -> Option<UiEvent> {
    let unread = unread.max(0);
    store(db, |conn| cache::set_unread(conn, peer_id, unread));
    Some(UiEvent::ReadInbox { peer_id, unread })
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
