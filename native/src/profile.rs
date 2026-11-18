//! The profile column of the open chat (click on the chat header): big
//! avatar, title, subtitle, username, about, phone and the mute switch.
//! Content follows `src/views/InfoPanel.svelte` of the WebView build.

use oxigram_core::chats::{self, PeerInfo};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{AppWindow, ProfileLine, format, run, tg, ui, with_app};

/// The column's avatar is drawn at this size, in logical pixels.
const PHOTO_PX: f32 = 120.0;

const MONTHS: [&str; 12] = [
    "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября", "октября", "ноября", "декабря",
];

pub fn wire(window: &AppWindow) {
    window.on_open_info(|| {
        if let Some(ui) = ui() {
            ui.set_info_open(true);
        }
        load();
    });
}

/// Another chat was opened: the column, if open, shows it.
pub fn chat_changed() {
    if ui().is_some_and(|ui| ui.get_info_open()) {
        load();
    }
}

/// Fills the column from the chat's list row at once, then from the full profile.
fn load() {
    let Some(Some((peer, generation))) = with_app(|app| {
        let peer = app.chat?;
        let ui = app.ui.upgrade()?;
        let dialog = app.dialog(peer);
        let title = dialog.map(format::dialog_title).unwrap_or_default();
        let saved = dialog.is_some_and(|d| d.kind == "saved");
        ui.set_info_initial(format::initial(&title).into());
        ui.set_info_title(title.into());
        ui.set_info_color(format::avatar_color(peer));
        ui.set_info_saved(saved);
        ui.set_info_can_mute(!saved);
        ui.set_info_subtitle(dialog.map(|d| basic_subtitle(d)).unwrap_or_default().into());
        ui.set_info_error(SharedString::new());
        ui.set_info_loading(true);
        ui.set_info_lines(ModelRc::new(VecModel::from(Vec::<ProfileLine>::new())));
        match app.avatars.get(&peer) {
            Some(photo) => {
                ui.set_info_photo(photo.clone());
                ui.set_info_has_photo(true);
            }
            None => ui.set_info_has_photo(false),
        }
        Some((peer, app.generation))
    }) else {
        return;
    };
    if let Some(photo_id) = with_app(|app| app.dialog(peer).and_then(|d| d.photo_id.clone())).flatten() {
        load_photo(peer, photo_id, generation);
    }
    run(async move { chats::peer_info(tg(), peer).await }, move |info| {
        with_app(|app| {
            if app.generation != generation {
                return;
            }
            let Some(ui) = app.ui.upgrade() else { return };
            ui.set_info_loading(false);
            match info {
                Ok(info) => {
                    ui.set_info_title(info.title.as_str().into());
                    ui.set_info_initial(format::initial(&info.title).into());
                    let (bot, seen) = app
                        .dialog(peer)
                        .map(|d| (d.bot, format::presence_text(d.presence.as_ref())))
                        .unwrap_or_default();
                    ui.set_info_subtitle(subtitle(&info, bot, seen).into());
                    ui.set_info_lines(ModelRc::new(VecModel::from(lines(&info))));
                    ui.set_info_can_mute(info.kind != "saved");
                    // Own profile or a photo the list did not know yet.
                    if let Some(photo_id) = info.photo_id.clone()
                        && !ui.get_info_has_photo()
                    {
                        load_photo(peer, photo_id, generation);
                    }
                }
                Err(err) => ui.set_info_error(format::error(&err).into()),
            }
        });
    });
}

/// The photo at the column's size (the list's one is too small).
fn load_photo(peer: i64, photo_id: String, generation: u64) {
    let scale = ui().map_or(1.0, |ui| ui.window().scale_factor());
    let px = (PHOTO_PX * scale).ceil() as u32;
    if !with_app(|app| app.settings.avatars).unwrap_or(true) {
        return;
    }
    run(
        async move {
            let path = chats::avatar(tg(), peer, &photo_id).await.ok()?;
            tokio::task::spawn_blocking(move || {
                let picture = crate::pictures::open_image(path).ok()?;
                let big = picture.resize_to_fill(px, px, image::imageops::FilterType::Triangle);
                Some(crate::circle(big.to_rgba8()))
            })
            .await
            .ok()
            .flatten()
        },
        move |pixels| {
            let Some(pixels) = pixels else { return };
            with_app(|app| {
                if app.generation != generation {
                    return;
                }
                let Some(ui) = app.ui.upgrade() else { return };
                let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                );
                ui.set_info_photo(slint::Image::from_rgba8(buffer));
                ui.set_info_has_photo(true);
            });
        },
    );
}

/// What is known before the profile arrives.
fn basic_subtitle(dialog: &oxigram_core::store::chats::DialogRow) -> String {
    match dialog.kind.as_str() {
        "saved" => "Избранное".to_owned(),
        "user" if dialog.bot => "бот".to_owned(),
        "user" => {
            let seen = format::presence_text(dialog.presence.as_ref());
            if seen.is_empty() { "Личный чат".to_owned() } else { seen }
        }
        "channel" => "Канал".to_owned(),
        _ => "Группа".to_owned(),
    }
}

fn subtitle(info: &PeerInfo, bot: bool, seen: String) -> String {
    let count = |n: i32| -> String {
        let digits = n.to_string();
        // Thousands separated by a no-break space, as `toLocaleString("ru")`.
        let mut out = String::new();
        for (i, ch) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push('\u{a0}');
            }
            out.push(ch);
        }
        out
    };
    match info.kind.as_str() {
        "saved" => "Избранное".to_owned(),
        "user" if bot => "бот".to_owned(),
        "user" if seen.is_empty() => "Личный чат".to_owned(),
        "user" => seen,
        kind => match info.members {
            None => if kind == "channel" { "Канал" } else { "Группа" }.to_owned(),
            Some(n) if kind == "channel" => {
                format!("{} {}", count(n), format::plural(n.into(), "подписчик", "подписчика", "подписчиков"))
            }
            Some(n) => {
                let text = format!("{} {}", count(n), format::plural(n.into(), "участник", "участника", "участников"));
                match info.online.filter(|o| *o > 0) {
                    Some(online) => format!("{text}, {} в сети", count(online)),
                    None => text,
                }
            }
        },
    }
}

fn birthday(value: &str) -> String {
    let mut parts = value.split('.');
    let (Some(day), Some(month)) = (parts.next(), parts.next()) else { return value.to_owned() };
    let name = month.parse::<usize>().ok().and_then(|m| MONTHS.get(m.wrapping_sub(1)));
    match (day.parse::<u32>(), name) {
        (Ok(day), Some(name)) => match parts.next() {
            Some(year) => format!("{day} {name} {year}"),
            None => format!("{day} {name}"),
        },
        _ => value.to_owned(),
    }
}

fn lines(info: &PeerInfo) -> Vec<ProfileLine> {
    let group_like = info.kind == "group" || info.kind == "channel";
    let line = |value: String, label: &str| ProfileLine { value: value.into(), label: label.into() };
    let mut rows = Vec::new();
    if let Some(phone) = info.phone.as_deref().filter(|p| !p.is_empty()) {
        let phone = phone.trim_start_matches('+');
        rows.push(line(format!("+{phone}"), "Телефон"));
    }
    if let Some(name) = info.username.as_deref().filter(|u| !u.is_empty()) {
        rows.push(line(format!("@{name}"), if group_like { "Ссылка" } else { "Имя пользователя" }));
    }
    if let Some(day) = info.birthday.as_deref().filter(|b| !b.is_empty()) {
        rows.push(line(birthday(day), "День рождения"));
    }
    if !info.about.trim().is_empty() {
        rows.push(line(info.about.trim().to_owned(), if group_like { "Описание" } else { "О себе" }));
    }
    if let Some(n) = info.common_chats.filter(|n| *n > 0) {
        rows.push(line(
            format!("{n} {}", format::plural(n.into(), "общая группа", "общие группы", "общих групп")),
            "Общие группы",
        ));
    }
    if let Some(link) = info.invite_link.as_deref().filter(|l| !l.is_empty()) {
        rows.push(line(link.to_owned(), "Ссылка-приглашение"));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::birthday;

    #[test]
    fn birthdays_read_in_russian() {
        assert_eq!(birthday("13.11"), "13 ноября");
        assert_eq!(birthday("05.01.1990"), "5 января 1990");
        assert_eq!(birthday("junk"), "junk");
    }
}
