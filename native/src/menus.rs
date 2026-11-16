//! Context menus of the chat list, the burger menu, settings, the confirm
//! dialog and the toast.

use std::future::Future;
use std::time::Duration;

use oxigram_core::actions;
use oxigram_core::chats;
use oxigram_core::error::UiError;
use oxigram_core::store::chats::DialogRow;
use oxigram_core::tg::Me;
use slint::ComponentHandle;

use crate::{AppWindow, format, open_chat, run, show_state, tg, ui, with_app};

const TOAST_FOR: Duration = Duration::from_millis(2500);
/// The drawer's avatar is drawn at this size, in logical pixels.
const ME_AVATAR_PX: f32 = 64.0;

pub fn wire(window: &AppWindow) {
    window.on_confirm_done(|accepted| {
        if let Some(ui) = ui() {
            ui.set_confirm_open(false);
            // Called after the borrow ends: the closure uses `with_app` itself.
            let then = with_app(|app| app.pending.take()).flatten();
            if let (true, Some(then)) = (accepted, then) {
                then(ui.get_confirm_checked());
            }
        }
    });
    window.on_open_saved(|| {
        let saved = with_app(|app| app.dialogs.iter().find(|d| d.kind == "saved").map(|d| d.id)).flatten();
        if let Some(peer) = saved {
            open_chat(peer);
        }
    });
    window.on_reload_dialogs(|| {
        with_app(|app| app.dialogs_done = false);
        crate::load_dialogs();
    });
    window.on_open_settings(|| {
        if let Some(ui) = ui() {
            ui.set_cache_text(format_size(tg().cache_size()).into());
            crate::settings::fill(&ui);
            ui.set_settings_page("main".into());
            ui.set_settings_open(true);
        }
    });
    window.on_clear_cache(|| {
        tg().clear_cache();
        if let Some(ui) = ui() {
            ui.set_cache_text(format_size(tg().cache_size()).into());
        }
        toast("Кэш очищен");
    });
    window.on_ask_sign_out(|| {
        confirm(
            "Выйти из аккаунта?",
            "Сессия и кэш на этом компьютере будут удалены.",
            "Выйти",
            "",
            |_| sign_out(),
        );
    });
}

/// Shows a short note at the bottom; it goes away on its own unless a newer
/// note has replaced it by then.
pub fn toast(text: &str) {
    let Some(ui) = ui() else { return };
    ui.set_toast(text.into());
    let text = text.to_owned();
    let weak = ui.as_weak();
    slint::Timer::single_shot(TOAST_FOR, move || {
        if let Some(ui) = weak.upgrade()
            && ui.get_toast() == text.as_str()
        {
            ui.set_toast(Default::default());
        }
    });
}

/// Asks before something destructive. `then` runs only when accepted, with the
/// state of the checkbox (hidden when `check` is empty).
pub fn confirm(title: &str, text: &str, ok: &str, check: &str, then: impl FnOnce(bool) + 'static) {
    let Some(ui) = ui() else { return };
    ui.set_confirm_title(title.into());
    ui.set_confirm_text(text.into());
    ui.set_confirm_ok(ok.into());
    ui.set_confirm_check(check.into());
    ui.set_confirm_checked(false);
    ui.set_confirm_open(true);
    with_app(|app| app.pending = Some(Box::new(then)));
}

/// Fills the burger menu and settings once the account is known.
pub fn show_me(me: &Me) {
    let Some(ui) = ui() else { return };
    let digits: String = me.phone.as_deref().unwrap_or("").chars().filter(char::is_ascii_digit).collect();
    ui.set_me_name(me.name.clone().into());
    ui.set_me_phone(if digits.is_empty() { String::new() } else { format!("+{digits}") }.into());
    ui.set_me_initial(format::initial(&me.name).into());
    ui.set_me_color(format::avatar_color(me.id));
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    ui.set_me_username(me.username.as_deref().filter(|u| !u.is_empty()).map(|u| format!("@{u}")).unwrap_or_default().into());
    ui.set_me_account(match me.username.as_deref().filter(|u| !u.is_empty()) {
        Some(user) => format!("{} · @{user}", me.name),
        None => me.name.clone(),
    }
    .into());
    let fetch = with_app(|app| {
        let changed = app.me_id != me.id || app.me_photo_id != me.photo_id;
        app.me_id = me.id;
        app.me_photo_id = me.photo_id.clone();
        changed
    });
    let avatars = with_app(|app| app.settings.avatars).unwrap_or(true);
    match (&me.photo_id, fetch) {
        (Some(photo_id), Some(true)) if avatars => load_me_photo(me.id, photo_id.clone(), &ui),
        (None, _) => ui.set_me_has_photo(false),
        _ => {}
    }
}

/// Fetches the own photo again after "Фото на аватарках" is switched back on.
pub fn reload_me_photo() {
    let known = with_app(|app| app.me_photo_id.clone().map(|p| (app.me_id, p))).flatten();
    if let (Some((id, photo_id)), Some(ui)) = (known, ui()) {
        load_me_photo(id, photo_id, &ui);
    }
}

fn load_me_photo(id: i64, photo_id: String, window: &AppWindow) {
    let px = (ME_AVATAR_PX * window.window().scale_factor()).ceil() as u32;
    run(
        async move {
            let path = chats::avatar(tg(), id, &photo_id).await.ok()?;
            tokio::task::spawn_blocking(move || {
                let picture = image::open(path).ok()?;
                let small = picture.resize_to_fill(px, px, image::imageops::FilterType::Triangle);
                Some(crate::circle(small.to_rgba8()))
            })
            .await
            .ok()
            .flatten()
        },
        |pixels| {
            let (Some(pixels), Some(ui)) = (pixels, ui()) else { return };
            let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                pixels.as_raw(),
                pixels.width(),
                pixels.height(),
            );
            ui.set_me_photo(slint::Image::from_rgba8(buffer));
            ui.set_me_has_photo(true);
        },
    );
}

/// "12,3 МБ", "512 КБ".
pub(crate) fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB * KB {
        format!("{:.1} ГБ", b / (KB * KB * KB)).replace('.', ",")
    } else if b >= KB * KB {
        format!("{:.1} МБ", b / (KB * KB)).replace('.', ",")
    } else {
        format!("{} КБ", (b / KB).round() as u64)
    }
}

/// Runs a backend call for a dialog; on success `apply` updates the view, on
/// failure the error is toasted.
fn act(
    work: impl Future<Output = Result<(), UiError>> + Send + 'static,
    apply: impl FnOnce() + Send + 'static,
) {
    run(work, move |result| match result {
        Ok(()) => apply(),
        Err(err) => toast(&format::error(&err)),
    });
}

/// Changes one dialog locally and redraws the list.
fn update(peer: i64, change: impl FnOnce(&mut DialogRow)) {
    with_app(|app| {
        if let Some(dialog) = app.dialogs.iter_mut().find(|d| d.id == peer) {
            change(dialog);
        }
        app.sort_dialogs();
        app.refresh_dialogs();
    });
}

pub(crate) fn dialog_action(peer: i64, name: &str) {
    if name == "open" {
        open_chat(peer);
        return;
    }
    let Some(Some((pinned, archived, muted, kind, title))) = with_app(|app| {
        app.dialog(peer)
            .map(|d| (d.pinned, d.archived, d.muted, d.kind.clone(), format::dialog_title(d)))
    }) else {
        return;
    };
    match name {
        "pin" => act(
            async move { actions::pin_dialog(tg(), peer, !pinned).await },
            move || update(peer, |d| d.pinned = !pinned),
        ),
        "read" => act(
            async move { chats::mark_read(tg(), peer).await },
            move || update(peer, |d| d.unread = 0),
        ),
        "unread" => act(
            async move { actions::mark_unread(tg(), peer, true).await },
            move || update(peer, |d| d.unread = d.unread.max(1)),
        ),
        "archive" => act(
            async move { actions::archive(tg(), peer, !archived).await },
            move || update(peer, |d| d.archived = !archived),
        ),
        "mute" => act(
            async move { chats::set_muted(tg(), peer, !muted).await },
            move || update(peer, |d| d.muted = !muted),
        ),
        "clear" => confirm(
            "Очистить историю?",
            &format!("Сообщения в чате «{title}» будут удалены у вас. Это нельзя отменить."),
            "Очистить",
            "",
            move |_| {
                act(async move { actions::clear_history(tg(), peer).await }, move || {
                    with_app(|app| {
                        if app.chat == Some(peer) {
                            app.set_messages(Vec::new());
                        }
                    });
                    update(peer, |d| d.last_text.clear());
                });
            },
        ),
        "delete" => {
            let (heading, ok) = match kind.as_str() {
                "group" => ("Покинуть группу?", "Покинуть"),
                "channel" => ("Покинуть канал?", "Покинуть"),
                _ => ("Удалить чат?", "Удалить"),
            };
            confirm(
                heading,
                &format!("«{title}» пропадёт из списка чатов."),
                ok,
                "",
                move |_| {
                    act(async move { actions::delete_chat(tg(), peer).await }, move || {
                        let was_open = with_app(|app| {
                            app.dialogs.retain(|d| d.id != peer);
                            let open = app.chat == Some(peer);
                            if open {
                                app.chat = None;
                                // Late replies for this chat are dropped.
                                app.generation += 1;
                                app.set_messages(Vec::new());
                                if let Some(ui) = app.ui.upgrade() {
                                    ui.set_chat_open(false);
                                }
                            }
                            app.refresh_dialogs();
                            open
                        });
                        if was_open == Some(true) {
                            tg().ui.set_active_chat(None);
                        }
                    });
                },
            );
        }
        _ => {}
    }
}

fn sign_out() {
    run(async { tg().sign_out().await }, |result| match result {
        Ok(state) => {
            with_app(|app| {
                app.dialogs.clear();
                app.shown.clear();
                app.dialog_model.clear();
                app.avatars.clear();
                app.avatar_order.clear();
                app.avatar_pending.clear();
                app.chat = None;
                app.generation += 1;
                app.dialogs_done = false;
                app.dialogs_loading = false;
                app.me_id = 0;
                app.me_photo_id = None;
                app.set_messages(Vec::new());
                if let Some(ui) = app.ui.upgrade() {
                    ui.set_chat_open(false);
                    ui.set_settings_open(false);
                    ui.set_drawer_open(false);
                    ui.set_me_has_photo(false);
                    // `show_state` attaches the live events again on the next login.
                    ui.set_stage("loading".into());
                }
            });
            tg().ui.set_active_chat(None);
            show_state(Ok(state));
        }
        Err(err) => toast(&format::error(&err)),
    });
}

#[cfg(test)]
mod tests {
    use super::format_size;

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(format_size(0), "0 КБ");
        assert_eq!(format_size(512 * 1024), "512 КБ");
        assert_eq!(format_size(12_897_484), "12,3 МБ");
    }
}
