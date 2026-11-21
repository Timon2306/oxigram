//! Text the window shows: times, dates, previews, error messages. Mirrors the
//! helpers in `src/lib/ipc.ts` of the WebView build.

use chrono::{DateTime, Datelike, Local, TimeZone};
use oxigram_core::error::UiError;
use oxigram_core::store::chats::{DialogRow, MessageRow, Presence};
use slint::Color;

const AVATAR_COLORS: [u32; 7] = [
    0xe17076, 0x7bc862, 0xe5ca77, 0x65aadd, 0xa695e7, 0xee7aae, 0x6ec9cb,
];
/// Name colours by sender, Telegram's order, tuned for the dark background.
const NAME_COLORS: [u32; 7] = [
    0xff7a7a, 0xffa94d, 0xb794f6, 0x6fd388, 0x4fd1d9, 0x6ab3f3, 0xff85b8,
];

fn rgb(hex: u32) -> Color {
    Color::from_rgb_u8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub fn avatar_color(peer_id: i64) -> Color {
    rgb(AVATAR_COLORS[(peer_id.unsigned_abs() % 7) as usize])
}

pub fn name_color(sender_id: i64) -> Color {
    rgb(NAME_COLORS[(sender_id.unsigned_abs() % 7) as usize])
}

pub fn initial(title: &str) -> String {
    title
        .trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".to_owned())
}

pub fn dialog_title(dialog: &DialogRow) -> String {
    if dialog.kind == "saved" {
        "Saved Messages".to_owned()
    } else if dialog.title.is_empty() {
        "Untitled".to_owned()
    } else {
        dialog.title.clone()
    }
}

fn local(unix: i64) -> Option<DateTime<Local>> {
    Local.timestamp_opt(unix, 0).single()
}

pub fn time(unix: i64) -> String {
    local(unix).map(|t| t.format("%H:%M").to_string()).unwrap_or_default()
}

/// Time for today, a short date otherwise: the dialog list convention.
pub fn list_date(unix: i64) -> String {
    if unix == 0 {
        return String::new();
    }
    let Some(t) = local(unix) else { return String::new() };
    if t.date_naive() == Local::now().date_naive() {
        t.format("%H:%M").to_string()
    } else {
        t.format("%d.%m.%y").to_string()
    }
}

pub fn same_day(a: i64, b: i64) -> bool {
    match (local(a), local(b)) {
        (Some(a), Some(b)) => a.date_naive() == b.date_naive(),
        _ => false,
    }
}

/// "November 15", with the year when it is not this one.
pub fn day(unix: i64) -> String {
    const MONTHS: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December",
    ];
    let Some(t) = local(unix) else { return String::new() };
    let month = MONTHS[t.month0() as usize];
    if t.year() == Local::now().year() {
        format!("{month} {}", t.day())
    } else {
        format!("{month} {}, {}", t.day(), t.year())
    }
}

pub fn plural<'a>(n: i64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let (tens, ones) = (n % 100, n % 10);
    if (11..=14).contains(&tens) {
        many
    } else if ones == 1 {
        one
    } else if (2..=4).contains(&ones) {
        few
    } else {
        many
    }
}

pub fn now() -> i64 {
    Local::now().timestamp()
}

/// Online right now (an "online" status expires on its own).
pub fn is_online(presence: Option<&Presence>) -> bool {
    presence.is_some_and(|p| p.kind == "online" && p.at > now())
}

/// "online", "last seen at 14:05", "last seen recently"… as Telegram words it.
pub fn presence_text(presence: Option<&Presence>) -> String {
    let Some(presence) = presence else { return String::new() };
    if is_online(Some(presence)) {
        return "online".to_owned();
    }
    match presence.kind {
        "recently" => return "last seen recently".to_owned(),
        "week" => return "last seen within a week".to_owned(),
        "month" => return "last seen within a month".to_owned(),
        "hidden" => return "last seen a long time ago".to_owned(),
        _ => {}
    }
    let ago = now() - presence.at;
    if ago < 60 {
        return "last seen just now".to_owned();
    }
    if ago < 3600 {
        let minutes = ago / 60;
        return format!(
            "last seen {minutes} {} ago",
            plural(minutes, "minute", "minutes", "minutes")
        );
    }
    let (Some(seen), today) = (local(presence.at), Local::now().date_naive()) else {
        return String::new();
    };
    let time = seen.format("%H:%M");
    if seen.date_naive() == today {
        format!("last seen today at {time}")
    } else if today.pred_opt() == Some(seen.date_naive()) {
        format!("last seen yesterday at {time}")
    } else {
        format!("last seen {}", seen.format("%d.%m.%y"))
    }
}

pub fn kind_label(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "text" | "service" => return None,
        "photo" => "Photo",
        "sticker" => "Sticker",
        "document" => "File",
        "video" => "Video",
        "gif" => "GIF",
        "round" => "Video message",
        "voice" => "Voice message",
        "audio" => "Audio",
        "contact" => "Contact",
        "poll" => "Poll",
        "location" => "Location",
        "dice" => "Dice",
        _ => "Attachment",
    })
}

/// Dialog previews arrive as "[kind]" when the message has no text.
pub fn preview_text(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default();
    let kind = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']'));
    // Only "[photo]"-style markers; "[текст в скобках]" stays text.
    if let Some(kind) = kind.filter(|k| !k.is_empty() && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')) {
        if let Some(label) = kind_label(kind) {
            return label.to_owned();
        }
    }
    line.to_owned()
}

/// The dialog-list line for a message that just arrived.
pub fn message_preview(message: &MessageRow) -> String {
    if !message.text.trim().is_empty() {
        return message.text.lines().next().unwrap_or_default().to_owned();
    }
    format!("[{}]", message.kind)
}

pub fn error(err: &UiError) -> String {
    if let Some(seconds) = err.wait_seconds {
        let wait = if seconds < 60 {
            format!("{seconds} s")
        } else if seconds < 3600 {
            format!("{} min", seconds.div_ceil(60))
        } else {
            format!("{} h", seconds.div_ceil(3600))
        };
        return format!("Telegram asks you to wait {wait} before trying again.");
    }
    let known = match err.code.as_str() {
        "KEYS_INVALID" => "Check the keys: api_id is a number, api_hash is 32 characters (0-9, a-f).",
        "API_ID_INVALID" => "Telegram did not accept this api_id / api_hash. Check them and enter them again.",
        "API_ID_PUBLISHED_FLOOD" => "This api_id is restricted by Telegram. A different key is needed.",
        "PHONE_NUMBER_INVALID" => "Invalid phone number. Enter it in international format.",
        "PHONE_NUMBER_BANNED" => "This number is banned in Telegram.",
        "PHONE_NUMBER_FLOOD" => "Too many sign-in attempts with this number. Try again later.",
        "PHONE_CODE_INVALID" => "Invalid code.",
        "PHONE_CODE_EXPIRED" => "The code has expired. Request a new one.",
        "PHONE_CODE_EMPTY" => "Enter the code.",
        "PASSWORD_INVALID" => "Invalid password.",
        "SIGN_UP_REQUIRED" => {
            "There is no account with this number. You can only sign up in the official Telegram app."
        }
        "NETWORK" => "Can't reach Telegram. Check your internet connection and try again.",
        "NOT_AUTHORIZED" => "You are not signed in.",
        "AUTH_KEY_UNREGISTERED" => "The session was ended on the server. Sign in again.",
        "PEER_UNKNOWN" => "This chat is not available yet. Refresh the chat list.",
        "MESSAGE_EMPTY" => "The message is empty.",
        "MESSAGE_TOO_LONG" => "The message is longer than 4096 characters.",
        "CHAT_WRITE_FORBIDDEN" => "You can't write in this chat.",
        "USER_IS_BLOCKED" => "This user has blocked you.",
        "WRONG_STAGE" => "This sign-in step is no longer valid. Start signing in again.",
        "ALREADY_RUNNING" => "Oxigram is already running with this account. Close the other window (and the tray icon).",
        _ => return format!("{} ({})", err.message, err.code),
    };
    known.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_name_media() {
        assert_eq!(preview_text("[photo]"), "Photo");
        assert_eq!(preview_text("привет\nкак дела"), "привет");
        assert_eq!(preview_text("[не тип]"), "[не тип]");
    }

    #[test]
    fn plurals_follow_russian_rules() {
        assert_eq!(plural(1, "чат", "чата", "чатов"), "чат");
        assert_eq!(plural(3, "чат", "чата", "чатов"), "чата");
        assert_eq!(plural(12, "чат", "чата", "чатов"), "чатов");
        assert_eq!(plural(21, "чат", "чата", "чатов"), "чат");
    }

    #[test]
    fn initials_are_uppercase() {
        assert_eq!(initial("  анна"), "А");
        assert_eq!(initial(""), "?");
    }
}
