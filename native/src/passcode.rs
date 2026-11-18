//! The local passcode in the window: the lock screen, "Код-пароль" in the
//! settings, auto-lock after idle time, Ctrl+L and the tray's "Заблокировать".
//! The encryption itself is in `oxigram_core::vault` and `Telegram`.

use std::cell::Cell;
use std::time::{Duration, Instant};

use slint::SharedString;

use crate::{AppWindow, format, login_step, run, tg, ui, with_app};

/// Choices of "Автоблокировка", in minutes (0 = off).
const AUTO_LOCK: [u32; 5] = [0, 5, 15, 60, 300];
const IDLE_CHECK: Duration = Duration::from_secs(20);

thread_local! {
    static LAST_INPUT: Cell<Instant> = Cell::new(Instant::now());
    static IDLE_TIMER: slint::Timer = slint::Timer::default();
}

/// Keyboard or mouse input in the window.
pub fn touch() {
    LAST_INPUT.with(|t| t.set(Instant::now()));
}

pub fn auto_lock_label(minutes: u32) -> String {
    match minutes {
        0 => "Выкл.".to_owned(),
        m if m < 60 => format!("через {m} мин"),
        m => format!("через {} ч", m / 60),
    }
}

pub fn wire(window: &AppWindow) {
    window.on_unlock(|passcode| {
        if let Some(ui) = ui() {
            ui.set_busy(true);
            ui.set_error(SharedString::new());
        }
        run(async move { tg().unlock(&passcode).await }, |result| match result {
            Ok(()) => login_step(async { tg().state().await }),
            Err(err) => {
                if let Some(ui) = ui() {
                    ui.set_busy(false);
                    ui.set_error(unlock_error(&err).into());
                }
            }
        });
    });
    window.on_forgot_passcode(|| {
        crate::menus::confirm(
            "Забыли код-пароль?",
            "Его нельзя восстановить: зашифрованные данные Oxigram на этом компьютере будут удалены, и придётся заново ввести ключи API и войти. Аккаунт и переписка в Telegram не пострадают; этот вход останется в списке активных сеансов, пока его не завершить с другого устройства.",
            "Удалить данные",
            "",
            |_| login_step(async { tg().forget_passcode().await }),
        );
    });
    window.on_lock_now(lock_now);
    window.on_passcode_enable(|first, second| {
        if first != second {
            status("Коды не совпадают");
            return;
        }
        if !oxigram_core::vault::valid_passcode(&first) {
            status("Нужно хотя бы 4 символа");
            return;
        }
        busy(true, "Шифрование…");
        run(async move { tg().enable_passcode(&first).await }, |result| {
            done(result, "Код-пароль включён. Данные на диске зашифрованы.");
            crate::drafts::forget_file();
        });
    });
    window.on_passcode_change(|old, first, second| {
        if first != second {
            status("Новые коды не совпадают");
            return;
        }
        busy(true, "Смена кода…");
        run(async move { tg().change_passcode(&old, &first).await }, |result| done(result, "Код-пароль изменён"));
    });
    window.on_passcode_disable(|passcode| {
        busy(true, "Расшифровка…");
        run(async move { tg().disable_passcode(&passcode).await }, |result| {
            done(result, "Код-пароль выключен");
            crate::drafts::save_now();
        });
    });
    window.on_cycle_auto_lock(|| {
        let minutes = with_app(|app| {
            let at = AUTO_LOCK.iter().position(|m| *m == app.settings.auto_lock).unwrap_or(0);
            app.settings.auto_lock = AUTO_LOCK[(at + 1) % AUTO_LOCK.len()];
            app.settings.save();
            app.settings.auto_lock
        })
        .unwrap_or(0);
        if let Some(ui) = ui() {
            ui.set_auto_lock_text(auto_lock_label(minutes).into());
        }
    });
    refresh(window);
    IDLE_TIMER.with(|timer| timer.start(slint::TimerMode::Repeated, IDLE_CHECK, check_idle));
}

/// The settings page's state.
pub fn refresh(window: &AppWindow) {
    window.set_passcode_on(tg().passcode_on());
    let minutes = with_app(|app| app.settings.auto_lock).unwrap_or(0);
    window.set_auto_lock_text(auto_lock_label(minutes).into());
}

fn status(text: &str) {
    if let Some(ui) = ui() {
        ui.set_passcode_status(text.into());
    }
}

fn busy(on: bool, text: &str) {
    if let Some(ui) = ui() {
        ui.set_passcode_busy(on);
        ui.set_passcode_status(text.into());
    }
}

fn done(result: Result<(), oxigram_core::error::UiError>, ok: &str) {
    let Some(ui) = ui() else { return };
    ui.set_passcode_busy(false);
    match result {
        Ok(()) => {
            ui.set_passcode_status(ok.into());
            refresh(&ui);
        }
        Err(err) => ui.set_passcode_status(unlock_error(&err).into()),
    }
}

fn unlock_error(err: &oxigram_core::error::UiError) -> String {
    match err.code.as_str() {
        "PASSCODE_WRONG" => "Неверный код-пароль".to_owned(),
        "PASSCODE_WAIT" => format!(
            "Слишком много неверных попыток. Подождите {} с",
            err.wait_seconds.unwrap_or(1)
        ),
        "PASSCODE_SHORT" => "Нужно хотя бы 4 символа".to_owned(),
        "PASSCODE_DAMAGED" => "Зашифрованные данные повреждены".to_owned(),
        _ => format::error(err),
    }
}

/// Locks now (Ctrl+L, the tray, auto-lock). Without a passcode, the
/// settings page that sets one opens instead.
pub fn lock_now() {
    if !tg().passcode_on() {
        if let Some(ui) = ui() {
            crate::settings::fill(&ui);
            ui.set_settings_page("passcode".into());
            ui.set_settings_open(true);
        }
        return;
    }
    if tg().is_locked() {
        return;
    }
    // What was decrypted goes from the window first.
    crate::forget_view();
    login_step(async { tg().lock().await });
}

fn check_idle() {
    let minutes = with_app(|app| app.settings.auto_lock).unwrap_or(0);
    if minutes == 0 || !tg().passcode_on() || tg().is_locked() {
        return;
    }
    let idle = LAST_INPUT.with(Cell::get).elapsed();
    if idle >= Duration::from_secs(u64::from(minutes) * 60) {
        lock_now();
    }
}
