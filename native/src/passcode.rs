//! The local passcode in the window: the lock screen, "Код-пароль" in the
//! settings, auto-lock after idle time, Ctrl+L and the tray's "Заблокировать".
//! The encryption itself is in `oxigram_core::vault` and `Telegram`.

use std::cell::Cell;
use std::time::{Duration, Instant};

use oxigram_core::tr;
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
        0 => tr!("Off"),
        m if m < 60 => tr!("after {} min", m),
        m => tr!("after {} h", m / 60),
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
            &tr!("Forgot the passcode?"),
            &tr!("It cannot be recovered: the encrypted Oxigram data on this computer will be deleted, and you will have to enter the API keys and sign in again. Your Telegram account and chats are not affected; this sign-in stays in the list of active sessions until you end it from another device."),
            &tr!("Delete Data"),
            "",
            |_| login_step(async { tg().forget_passcode().await }),
        );
    });
    window.on_lock_now(lock_now);
    window.on_passcode_enable(|first, second| {
        if first != second {
            status(&tr!("The codes do not match"));
            return;
        }
        if !oxigram_core::vault::valid_passcode(&first) {
            status(&tr!("At least 4 characters are needed"));
            return;
        }
        busy(true, &tr!("Encrypting…"));
        run(async move { tg().enable_passcode(&first).await }, |result| {
            done(result, &tr!("Passcode turned on. The data on disk is encrypted."));
            crate::drafts::forget_file();
        });
    });
    window.on_passcode_change(|old, first, second| {
        if first != second {
            status(&tr!("The new codes do not match"));
            return;
        }
        busy(true, &tr!("Changing the code…"));
        run(async move { tg().change_passcode(&old, &first).await }, |result| done(result, &tr!("Passcode changed")));
    });
    window.on_passcode_disable(|passcode| {
        busy(true, &tr!("Decrypting…"));
        run(async move { tg().disable_passcode(&passcode).await }, |result| {
            done(result, &tr!("Passcode turned off"));
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
        "PASSCODE_WRONG" => tr!("Wrong passcode"),
        "PASSCODE_WAIT" => tr!(
            "Too many wrong attempts. Wait {} s",
            err.wait_seconds.unwrap_or(1)
        ),
        "PASSCODE_SHORT" => tr!("At least 4 characters are needed"),
        "PASSCODE_DAMAGED" => tr!("The encrypted data is damaged"),
        _ => format::error(err),
    }
}

/// Locks now (Ctrl+L, the tray, auto-lock). Without a passcode there is
/// nothing to lock, and the tray must not drop the user into the settings:
/// it says so and stays where it is.
pub fn lock_now() {
    if !tg().passcode_on() {
        crate::menus::toast(tr!("Set a passcode first"));
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
