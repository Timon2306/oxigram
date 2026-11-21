//! Text the window shows: times, dates, previews, error messages. Mirrors the
//! helpers in `src/lib/ipc.ts` of the WebView build.

use chrono::{DateTime, Datelike, Local, TimeZone};
use oxigram_core::error::UiError;
use oxigram_core::store::chats::{DialogRow, MessageRow, Presence};
use oxigram_core::{tr, trn};
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
        tr!("Saved Messages")
    } else if dialog.title.is_empty() {
        tr!("Untitled")
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
    let Some(t) = local(unix) else { return String::new() };
    let year = (t.year() != Local::now().year()).then_some(t.year());
    month_day(t.month0(), t.day(), year)
}

/// "November 15" / "November 15, 2023" for a month number (0-11).
pub fn month_day(month0: u32, d: u32, year: Option<i32>) -> String {
    let (this_year, y) = (year.is_none(), year.unwrap_or_default());
    match month0 {
        0 => if this_year { tr!("January {}", d) } else { tr!("January {}, {}", d, y) },
        1 => if this_year { tr!("February {}", d) } else { tr!("February {}, {}", d, y) },
        2 => if this_year { tr!("March {}", d) } else { tr!("March {}, {}", d, y) },
        3 => if this_year { tr!("April {}", d) } else { tr!("April {}, {}", d, y) },
        4 => if this_year { tr!("May {}", d) } else { tr!("May {}, {}", d, y) },
        5 => if this_year { tr!("June {}", d) } else { tr!("June {}, {}", d, y) },
        6 => if this_year { tr!("July {}", d) } else { tr!("July {}, {}", d, y) },
        7 => if this_year { tr!("August {}", d) } else { tr!("August {}, {}", d, y) },
        8 => if this_year { tr!("September {}", d) } else { tr!("September {}, {}", d, y) },
        9 => if this_year { tr!("October {}", d) } else { tr!("October {}, {}", d, y) },
        10 => if this_year { tr!("November {}", d) } else { tr!("November {}, {}", d, y) },
        11 => if this_year { tr!("December {}", d) } else { tr!("December {}, {}", d, y) },
        _ => String::new(),
    }
}

/// A number with the language's thousands separator: 12,345 / 12 345.
pub fn number(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let separator = if crate::i18n::current() == "en" { ',' } else { '\u{00A0}' };
    let mut out = String::new();
    if n < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(separator);
        }
        out.push(c);
    }
    out
}

/// The decimal separator of the language ("1.5 MB" / "1,5 МБ").
pub fn decimal(text: String) -> String {
    if crate::i18n::current() == "en" { text } else { text.replace('.', ",") }
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
        return tr!("online");
    }
    match presence.kind {
        "recently" => return tr!("last seen recently"),
        "week" => return tr!("last seen within a week"),
        "month" => return tr!("last seen within a month"),
        "hidden" => return tr!("last seen a long time ago"),
        _ => {}
    }
    let ago = now() - presence.at;
    if ago < 60 {
        return tr!("last seen just now");
    }
    if ago < 3600 {
        let minutes = ago / 60;
        return trn!("last seen {n} minute ago", "last seen {n} minutes ago", minutes);
    }
    let (Some(seen), today) = (local(presence.at), Local::now().date_naive()) else {
        return String::new();
    };
    let time = seen.format("%H:%M");
    if seen.date_naive() == today {
        tr!("last seen at {}", time)
    } else if today.pred_opt() == Some(seen.date_naive()) {
        tr!("last seen yesterday at {}", time)
    } else {
        tr!("last seen {}", seen.format("%d.%m.%y"))
    }
}

pub fn kind_label(kind: &str) -> Option<String> {
    Some(match kind {
        "text" | "service" => return None,
        "photo" => tr!("Photo"),
        "sticker" => tr!("Sticker"),
        "document" => tr!("File"),
        "video" => tr!("Video"),
        "gif" => "GIF".to_owned(),
        "round" => tr!("Video message"),
        "voice" => tr!("Voice message"),
        "audio" => tr!("Audio"),
        "contact" => tr!("Contact"),
        "poll" => tr!("Poll"),
        "location" => tr!("Location"),
        "dice" => tr!("Dice"),
        "paid" => tr!("Paid post"),
        "story" => tr!("Story"),
        "giveaway" => tr!("Giveaway"),
        "invoice" => tr!("Invoice"),
        "game" => tr!("Game"),
        "todo" => tr!("Checklist"),
        "unsupported" => tr!("Unsupported message"),
        _ => tr!("Attachment"),
    })
}

/// Kinds Oxigram cannot draw: the bubble offers the post in Telegram.
pub fn is_foreign(kind: &str) -> bool {
    matches!(kind, "paid" | "story" | "giveaway" | "invoice" | "game" | "todo" | "unsupported")
}

/// The post's address in Telegram (channels and supergroups only):
/// t.me/c/<channel>/<message>.
pub fn post_link(peer_id: i64, message_id: i32) -> Option<String> {
    let channel = -peer_id - 1_000_000_000_000;
    (channel > 0).then(|| format!("https://t.me/c/{channel}/{message_id}"))
}

/// Dialog previews arrive as "[kind]" when the message has no text.
pub fn preview_text(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default();
    let kind = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']'));
    // Only "[photo]"-style markers; "[text in brackets]" stays text.
    if let Some(kind) = kind.filter(|k| !k.is_empty() && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')) {
        // A post with nothing the list can show (older caches wrote "[text]").
        if kind == "text" {
            return tr!("Message");
        }
        if let Some(label) = kind_label(kind) {
            return label;
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
    // A session that is not signed in any more sends the window back to the
    // sign-in (wherever the error is shown).
    crate::menus::session_lost(&err.code);
    if let Some(seconds) = err.wait_seconds {
        let wait = if seconds < 60 {
            tr!("{} s", seconds)
        } else if seconds < 3600 {
            tr!("{} min", seconds.div_ceil(60))
        } else {
            tr!("{} h", seconds.div_ceil(3600))
        };
        return tr!("Telegram asks you to wait {} before trying again.", wait);
    }
    match err.code.as_str() {
        "KEYS_INVALID" => tr!("Check the keys: api_id is a number, api_hash is 32 characters (0–9, a–f)."),
        "API_ID_INVALID" => tr!("Telegram did not accept this api_id / api_hash. Check them and enter them again."),
        "API_ID_PUBLISHED_FLOOD" => tr!("This api_id is limited by Telegram. You need another key."),
        "PHONE_NUMBER_INVALID" => tr!("Invalid phone number. Enter it in international format."),
        "PHONE_NUMBER_BANNED" => tr!("This number is banned in Telegram."),
        "PHONE_NUMBER_FLOOD" => tr!("Too many sign-in attempts from this number. Try again later."),
        "PHONE_CODE_INVALID" => tr!("Invalid code."),
        "PHONE_CODE_EXPIRED" => tr!("The code has expired. Request a new one."),
        "PHONE_CODE_EMPTY" => tr!("Enter the code."),
        "PASSWORD_INVALID" => tr!("Wrong password."),
        "SIGN_UP_REQUIRED" => tr!("There is no account with this number. You can only sign up in the official Telegram app."),
        "NETWORK" => tr!("Can't reach Telegram. Check your internet and try again."),
        "NOT_AUTHORIZED" => tr!("You are not signed in."),
        "AUTH_KEY_UNREGISTERED" => tr!("The session was ended on the server. Sign in again."),
        "PEER_UNKNOWN" => tr!("This chat is not available yet. Refresh the chat list."),
        "MESSAGE_EMPTY" => tr!("The message is empty."),
        "MESSAGE_TOO_LONG" => tr!("The message is longer than 4096 characters."),
        "CHAT_WRITE_FORBIDDEN" => tr!("You can't write in this chat."),
        "USER_IS_BLOCKED" => tr!("The user has blocked you."),
        "WRONG_STAGE" => tr!("This sign-in step is no longer valid. Start signing in again."),
        "ALREADY_RUNNING" => tr!("Oxigram is already running with this account. Close the other window (and the tray icon)."),
        _ => return format!("{} ({})", err.message, err.code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_name_media() {
        assert_eq!(preview_text("[photo]"), "Photo");
        assert_eq!(preview_text("hello\nhow are you"), "hello");
        assert_eq!(preview_text("[not a kind]"), "[not a kind]");
    }

    #[test]
    fn initials_are_uppercase() {
        assert_eq!(initial("  anna"), "A");
        assert_eq!(initial(""), "?");
    }
}
