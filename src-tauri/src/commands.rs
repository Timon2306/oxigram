//! IPC surface: thin wrappers, all logic lives in the modules they call.
//!
//! The API hash never travels back to the frontend; `save_keys` is the only
//! command that carries it, and only inbound.

use tauri::State;
use tauri::ipc::Channel;

use crate::actions;
use crate::chats::{self, DialogPage, PeerInfo};
use crate::error::UiError;
use crate::store::chats::{DialogRow, MessageRow, Presence};
use crate::tg::{Telegram, UiState};
use crate::updates::UiEvent;

type Reply = Result<UiState, UiError>;

#[tauri::command]
pub async fn get_state(tg: State<'_, Telegram>) -> Reply {
    tg.state().await
}

#[tauri::command]
pub async fn save_keys(tg: State<'_, Telegram>, api_id: i32, api_hash: String) -> Reply {
    tg.save_keys(api_id, &api_hash).await
}

#[tauri::command]
pub async fn clear_keys(tg: State<'_, Telegram>) -> Reply {
    tg.clear_keys().await
}

#[tauri::command]
pub async fn request_code(tg: State<'_, Telegram>, phone: String) -> Reply {
    tg.request_code(&phone).await
}

#[tauri::command]
pub async fn submit_code(tg: State<'_, Telegram>, code: String) -> Reply {
    tg.submit_code(&code).await
}

#[tauri::command]
pub async fn submit_password(tg: State<'_, Telegram>, password: String) -> Reply {
    tg.submit_password(&password).await
}

#[tauri::command]
pub async fn back_to_phone(tg: State<'_, Telegram>) -> Reply {
    tg.back_to_phone().await
}

#[tauri::command]
pub async fn sign_out(tg: State<'_, Telegram>) -> Reply {
    tg.sign_out().await
}

#[tauri::command]
pub async fn cached_dialogs(tg: State<'_, Telegram>) -> Result<Vec<DialogRow>, UiError> {
    chats::cached_dialogs(&tg).await
}

#[tauri::command]
pub async fn load_dialogs(tg: State<'_, Telegram>, reset: bool) -> Result<DialogPage, UiError> {
    chats::load_dialogs(&tg, reset).await
}

#[tauri::command]
pub async fn cached_history(
    tg: State<'_, Telegram>,
    peer_id: i64,
    before_id: Option<i32>,
    limit: usize,
) -> Result<Vec<MessageRow>, UiError> {
    chats::cached_history(&tg, peer_id, before_id, limit).await
}

#[tauri::command]
pub async fn history(
    tg: State<'_, Telegram>,
    peer_id: i64,
    before_id: Option<i32>,
    limit: usize,
) -> Result<Vec<MessageRow>, UiError> {
    chats::history(&tg, peer_id, before_id, limit).await
}

#[tauri::command]
pub async fn send_message(
    tg: State<'_, Telegram>,
    peer_id: i64,
    text: String,
    reply_to: Option<i32>,
) -> Result<MessageRow, UiError> {
    chats::send_message(&tg, peer_id, &text, reply_to).await
}

type Done = Result<(), UiError>;

/// Sends a file whose bytes are the raw request body; the details travel in
/// headers (names URL-encoded), so no base64 copy of the file is ever made.
#[tauri::command]
pub async fn send_file(
    tg: State<'_, Telegram>,
    request: tauri::ipc::Request<'_>,
) -> Result<MessageRow, UiError> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(UiError::new("BAD_REQUEST", "expected file bytes"));
    };
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(decode_header)
            .unwrap_or_default()
    };
    let file = actions::OutgoingFile {
        peer_id: header("x-peer").parse().map_err(|_| UiError::new("BAD_REQUEST", "peer"))?,
        name: header("x-name"),
        caption: header("x-caption"),
        reply_to: header("x-reply-to").parse().ok(),
        as_photo: header("x-as-photo") == "1",
        token: header("x-token"),
    };
    actions::send_file(&tg, file, bytes.clone()).await
}

/// Undoes JavaScript's `encodeURIComponent` (headers must stay ASCII).
fn decode_header(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::decode_header;

    #[test]
    fn headers_decode_like_encode_uri_component() {
        assert_eq!(decode_header("%D0%A4%D0%BE%D1%82%D0%BE%20.jpg"), "Фото .jpg");
        assert_eq!(decode_header("plain"), "plain");
        assert_eq!(decode_header("50%"), "50%");
        assert_eq!(decode_header("%zz"), "%zz");
    }
}

/// Unread messages in unmuted chats, for the title bar and the tray tooltip.
#[tauri::command]
pub fn set_unread(app: tauri::AppHandle, unread: u32) {
    crate::mem::UNREAD.store(unread, std::sync::atomic::Ordering::Relaxed);
    crate::tray::set_unread(&app, unread);
}

/// The chat open in the window (or none), so its messages do not ring.
#[tauri::command]
pub fn set_active_chat(tg: State<'_, Telegram>, peer_id: Option<i64>) {
    tg.ui.set_active_chat(peer_id);
}

#[tauri::command]
pub async fn contacts(tg: State<'_, Telegram>) -> Result<Vec<actions::Contact>, UiError> {
    actions::contacts(&tg).await
}

#[tauri::command]
pub async fn resolve_username(
    tg: State<'_, Telegram>,
    username: String,
) -> Result<actions::FoundChat, UiError> {
    actions::resolve_username(&tg, &username).await
}

#[tauri::command]
pub async fn search_chat(
    tg: State<'_, Telegram>,
    peer_id: i64,
    query: String,
) -> Result<Vec<MessageRow>, UiError> {
    actions::search_chat(&tg, peer_id, &query, 50).await
}

#[tauri::command]
pub async fn search_global(
    tg: State<'_, Telegram>,
    query: String,
) -> Result<Vec<actions::FoundMessage>, UiError> {
    actions::search_global(&tg, &query, 30).await
}

#[tauri::command]
pub fn cache_size(tg: State<'_, Telegram>) -> u64 {
    tg.cache_size()
}

#[tauri::command]
pub fn clear_cache(tg: State<'_, Telegram>) -> u64 {
    tg.clear_cache();
    tg.cache_size()
}

#[tauri::command]
pub async fn folders(tg: State<'_, Telegram>, self_id: i64) -> Result<Vec<chats::Folder>, UiError> {
    chats::folders(&tg, self_id).await
}

#[tauri::command]
pub async fn set_typing(tg: State<'_, Telegram>, peer_id: i64) -> Done {
    actions::set_typing(&tg, peer_id).await
}

#[tauri::command]
pub async fn pin_dialog(tg: State<'_, Telegram>, peer_id: i64, pinned: bool) -> Done {
    actions::pin_dialog(&tg, peer_id, pinned).await
}

#[tauri::command]
pub async fn mark_unread(tg: State<'_, Telegram>, peer_id: i64, unread: bool) -> Done {
    actions::mark_unread(&tg, peer_id, unread).await
}

#[tauri::command]
pub async fn archive_dialog(tg: State<'_, Telegram>, peer_id: i64, archived: bool) -> Done {
    actions::archive(&tg, peer_id, archived).await
}

#[tauri::command]
pub async fn clear_history(tg: State<'_, Telegram>, peer_id: i64) -> Done {
    actions::clear_history(&tg, peer_id).await
}

#[tauri::command]
pub async fn delete_chat(tg: State<'_, Telegram>, peer_id: i64) -> Done {
    actions::delete_chat(&tg, peer_id).await
}

#[tauri::command]
pub async fn react(tg: State<'_, Telegram>, peer_id: i64, message_id: i32, emoji: String) -> Done {
    actions::react(&tg, peer_id, message_id, &emoji).await
}

#[tauri::command]
pub async fn edit_message(
    tg: State<'_, Telegram>,
    peer_id: i64,
    message_id: i32,
    text: String,
) -> Done {
    actions::edit_message(&tg, peer_id, message_id, &text).await
}

#[tauri::command]
pub async fn delete_messages(
    tg: State<'_, Telegram>,
    peer_id: i64,
    ids: Vec<i32>,
    for_everyone: bool,
) -> Done {
    actions::delete_messages(&tg, peer_id, &ids, for_everyone).await
}

#[tauri::command]
pub async fn pin_message(
    tg: State<'_, Telegram>,
    peer_id: i64,
    message_id: i32,
    pinned: bool,
) -> Done {
    actions::pin_message(&tg, peer_id, message_id, pinned).await
}

#[tauri::command]
pub async fn forward_messages(
    tg: State<'_, Telegram>,
    from_peer_id: i64,
    ids: Vec<i32>,
    to_peer_id: i64,
) -> Result<Vec<MessageRow>, UiError> {
    actions::forward(&tg, from_peer_id, &ids, to_peer_id).await
}

#[tauri::command]
pub async fn mark_read(tg: State<'_, Telegram>, peer_id: i64) -> Result<(), UiError> {
    chats::mark_read(&tg, peer_id).await
}

#[tauri::command]
pub async fn message_image(
    tg: State<'_, Telegram>,
    peer_id: i64,
    message_id: i32,
) -> Result<String, UiError> {
    chats::message_image(&tg, peer_id, message_id).await
}

#[tauri::command]
pub async fn avatar(
    tg: State<'_, Telegram>,
    peer_id: i64,
    photo_id: String,
) -> Result<String, UiError> {
    chats::avatar(&tg, peer_id, &photo_id).await
}

#[tauri::command]
pub async fn message_file(
    tg: State<'_, Telegram>,
    peer_id: i64,
    message_id: i32,
    on_progress: Channel<u64>,
) -> Result<String, UiError> {
    chats::message_file(&tg, peer_id, message_id, |bytes| {
        let _ = on_progress.send(bytes);
    })
    .await
}

/// Saves the attachment to the user's Downloads folder and returns its path.
#[tauri::command]
pub async fn save_file(
    app: tauri::AppHandle,
    tg: State<'_, Telegram>,
    peer_id: i64,
    message_id: i32,
) -> Result<String, UiError> {
    use tauri::Manager;
    let dir = app
        .path()
        .download_dir()
        .map_err(|err| UiError::new("NO_DOWNLOADS_DIR", err.to_string()))?;
    chats::save_file(&tg, peer_id, message_id, dir).await
}

#[tauri::command]
pub async fn shared_media(
    tg: State<'_, Telegram>,
    peer_id: i64,
    kind: String,
    before_id: Option<i32>,
    limit: usize,
) -> Result<Vec<MessageRow>, UiError> {
    chats::shared_media(&tg, peer_id, &kind, before_id, limit).await
}

#[tauri::command]
pub async fn set_muted(tg: State<'_, Telegram>, peer_id: i64, muted: bool) -> Result<(), UiError> {
    chats::set_muted(&tg, peer_id, muted).await
}

#[tauri::command]
pub async fn peer_presence(
    tg: State<'_, Telegram>,
    peer_id: i64,
) -> Result<Option<Presence>, UiError> {
    chats::peer_presence(&tg, peer_id).await
}

#[tauri::command]
pub async fn peer_info(tg: State<'_, Telegram>, peer_id: i64) -> Result<PeerInfo, UiError> {
    chats::peer_info(&tg, peer_id).await
}

/// Points the live event stream at this window. Called once per page load.
#[tauri::command]
pub async fn subscribe(tg: State<'_, Telegram>, on_event: Channel<Vec<UiEvent>>) -> Result<(), UiError> {
    tg.ui.attach(on_event);
    Ok(())
}

// `async` keeps the process snapshot off the main (UI) thread.
