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
        "Избранное".to_owned()
    } else if dialog.title.is_empty() {
        "Без названия".to_owned()
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

/// "15 ноября", with the year when it is not this one.
pub fn day(unix: i64) -> String {
    const MONTHS: [&str; 12] = [
        "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября",
        "октября", "ноября", "декабря",
    ];
    let Some(t) = local(unix) else { return String::new() };
    let month = MONTHS[t.month0() as usize];
    if t.year() == Local::now().year() {
        format!("{} {month}", t.day())
    } else {
        format!("{} {month} {}", t.day(), t.year())
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

/// "в сети", "был(а) в 14:05", "был(а) недавно"… as Telegram words it.
pub fn presence_text(presence: Option<&Presence>) -> String {
    let Some(presence) = presence else { return String::new() };
    if is_online(Some(presence)) {
        return "в сети".to_owned();
    }
    match presence.kind {
        "recently" => return "был(а) недавно".to_owned(),
        "week" => return "был(а) на этой неделе".to_owned(),
        "month" => return "был(а) в этом месяце".to_owned(),
        "hidden" => return "был(а) давно".to_owned(),
        _ => {}
    }
    let ago = now() - presence.at;
    if ago < 60 {
        return "был(а) только что".to_owned();
    }
    if ago < 3600 {
        let minutes = ago / 60;
        return format!(
            "был(а) {minutes} {} назад",
            plural(minutes, "минуту", "минуты", "минут")
        );
    }
    let (Some(seen), today) = (local(presence.at), Local::now().date_naive()) else {
        return String::new();
    };
    let time = seen.format("%H:%M");
    if seen.date_naive() == today {
        format!("был(а) в {time}")
    } else if today.pred_opt() == Some(seen.date_naive()) {
        format!("был(а) вчера в {time}")
    } else {
        format!("был(а) {}", seen.format("%d.%m.%y"))
    }
}

pub fn kind_label(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "text" | "service" => return None,
        "photo" => "Фото",
        "sticker" => "Стикер",
        "document" => "Файл",
        "video" => "Видео",
        "gif" => "GIF",
        "round" => "Видеосообщение",
        "voice" => "Голосовое сообщение",
        "audio" => "Аудио",
        "contact" => "Контакт",
        "poll" => "Опрос",
        "location" => "Геопозиция",
        "dice" => "Кубик",
        _ => "Вложение",
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
            format!("{seconds} с")
        } else if seconds < 3600 {
            format!("{} мин", seconds.div_ceil(60))
        } else {
            format!("{} ч", seconds.div_ceil(3600))
        };
        return format!("Telegram просит подождать {wait} перед следующей попыткой.");
    }
    let known = match err.code.as_str() {
        "KEYS_INVALID" => "Проверьте ключи: api_id — число, api_hash — 32 символа (0–9, a–f).",
        "API_ID_INVALID" => "Telegram не принял эти api_id / api_hash. Проверьте их и введите заново.",
        "API_ID_PUBLISHED_FLOOD" => "Этот api_id ограничен Telegram. Нужен другой ключ.",
        "PHONE_NUMBER_INVALID" => "Неверный номер телефона. Введите его в международном формате.",
        "PHONE_NUMBER_BANNED" => "Этот номер заблокирован в Telegram.",
        "PHONE_NUMBER_FLOOD" => "Слишком много попыток входа с этого номера. Попробуйте позже.",
        "PHONE_CODE_INVALID" => "Неверный код.",
        "PHONE_CODE_EXPIRED" => "Код устарел. Запросите новый.",
        "PHONE_CODE_EMPTY" => "Введите код.",
        "PASSWORD_INVALID" => "Неверный пароль.",
        "SIGN_UP_REQUIRED" => {
            "Аккаунта с этим номером нет. Зарегистрироваться можно только в официальном приложении Telegram."
        }
        "NETWORK" => "Нет связи с Telegram. Проверьте интернет и попробуйте ещё раз.",
        "NOT_AUTHORIZED" => "Вход в аккаунт не выполнен.",
        "AUTH_KEY_UNREGISTERED" => "Сессия завершена на сервере. Войдите заново.",
        "PEER_UNKNOWN" => "Этот чат пока недоступен. Обновите список диалогов.",
        "MESSAGE_EMPTY" => "Сообщение пустое.",
        "MESSAGE_TOO_LONG" => "Сообщение длиннее 4096 символов.",
        "CHAT_WRITE_FORBIDDEN" => "В этот чат нельзя писать.",
        "USER_IS_BLOCKED" => "Пользователь вас заблокировал.",
        "WRONG_STAGE" => "Этот шаг входа уже неактуален. Начните вход заново.",
        "ALREADY_RUNNING" => "Oxigram уже запущен с этим аккаунтом. Закройте другое окно (и значок в трее).",
        _ => return format!("{} ({})", err.message, err.code),
    };
    known.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_name_media() {
        assert_eq!(preview_text("[photo]"), "Фото");
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
