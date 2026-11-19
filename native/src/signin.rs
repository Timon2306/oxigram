//! The code and QR steps of the sign-in: how the code was sent, "send it
//! another way" with its countdown, and the QR code asked again every few
//! seconds until it is scanned.

use std::cell::Cell;
use std::time::{Duration, Instant};

use oxigram_core::login::CodeSent;
use oxigram_core::tr;
use oxigram_core::tg::UiState;

use crate::{AppWindow, run, show_state_quiet, tg, ui};

/// Telegram re-issues the token every half minute; asking more often than
/// every few seconds risks a flood wait.
const QR_EVERY: Duration = Duration::from_secs(5);

thread_local! {
    /// When "send another way" becomes available, and what it offers.
    static RESEND_AT: Cell<Option<Instant>> = const { Cell::new(None) };
    static NEXT: Cell<Option<&'static str>> = const { Cell::new(None) };
    static TICK: slint::Timer = slint::Timer::default();
    static QR_TIMER: slint::Timer = slint::Timer::default();
    static QR_BUSY: Cell<bool> = const { Cell::new(false) };
}

pub fn wire(window: &AppWindow) {
    window.on_resend_code(|| crate::login_step(async { tg().resend_code().await }));
    window.on_qr_start(|| crate::login_step(async { tg().qr_login().await }));
}

/// The line on the code screen: where to look for the code.
pub fn hint(sent: &CodeSent) -> String {
    let phone = &sent.phone;
    match sent.via.as_str() {
        "app" => tr!("The code was sent to the Telegram app on your other device ({}): look in the chat with Telegram.", phone),
        "call" => tr!("Telegram will call {} and dictate the code.", phone),
        "flash_call" | "missed_call" => tr!("Telegram will call {}: the code is the last digits of the calling number.", phone),
        "email" => tr!("The code was sent to your e-mail {}.", sent.hint.clone().unwrap_or_default()),
        "email_setup" => tr!("Telegram asks to set up a login e-mail first: do it in the official app, then try again."),
        "fragment" => tr!("The code was sent to your number on Fragment: {}", sent.hint.clone().unwrap_or_default()),
        _ => tr!("The code was sent by SMS to {}.", phone),
    }
}

fn next_label(next: &str) -> String {
    match next {
        "sms" => tr!("Send the code by SMS"),
        "call" => tr!("Call me with the code"),
        "flash_call" | "missed_call" => tr!("Get the code by a call"),
        "fragment" => tr!("Send the code via Fragment"),
        _ => tr!("Send the code another way"),
    }
}

/// Shows the code step: the hint and the resend countdown.
pub fn show_code(ui: &AppWindow, sent: &CodeSent) {
    ui.set_login_phone(sent.phone.clone().into());
    ui.set_code_hint(hint(sent).into());
    let next: Option<&'static str> = match sent.next.as_deref() {
        Some("sms") => Some("sms"),
        Some("call") => Some("call"),
        Some("flash_call") => Some("flash_call"),
        Some("missed_call") => Some("missed_call"),
        Some("fragment") => Some("fragment"),
        Some(_) => Some("other"),
        None => None,
    };
    NEXT.with(|n| n.set(next));
    let wait = Duration::from_secs(sent.timeout.unwrap_or(0).max(0) as u64);
    RESEND_AT.with(|r| r.set(next.map(|_| Instant::now() + wait)));
    tick();
    TICK.with(|t| t.start(slint::TimerMode::Repeated, Duration::from_secs(1), tick));
}

/// The resend button's text: "Send the code by SMS (0:42)" while waiting.
fn tick() {
    let Some(ui) = ui() else { return };
    let (Some(next), Some(at)) = (NEXT.with(Cell::get), RESEND_AT.with(Cell::get)) else {
        ui.set_resend_text("".into());
        TICK.with(slint::Timer::stop);
        return;
    };
    let left = at.saturating_duration_since(Instant::now()).as_secs();
    let label = next_label(next);
    if left > 0 {
        ui.set_resend_text(format!("{label} ({}:{:02})", left / 60, left % 60).into());
        ui.set_resend_ready(false);
    } else {
        ui.set_resend_text(label.into());
        ui.set_resend_ready(true);
        TICK.with(slint::Timer::stop);
    }
}

/// Shows the QR step and keeps asking until the code is scanned.
pub fn show_qr(ui: &AppWindow, path: &str, size: i32) {
    ui.set_qr_path(path.into());
    ui.set_qr_size(size.max(1));
    QR_TIMER.with(|t| {
        if !t.running() {
            t.start(slint::TimerMode::Repeated, QR_EVERY, poll_qr);
        }
    });
}

/// Leaving the QR step stops the polling.
pub fn left_qr() {
    QR_TIMER.with(slint::Timer::stop);
}

fn poll_qr() {
    if ui().is_none_or(|ui| ui.get_stage() != "qr") {
        left_qr();
        return;
    }
    if QR_BUSY.with(|b| b.replace(true)) {
        return;
    }
    run(async { tg().qr_login().await }, |result: Result<UiState, _>| {
        QR_BUSY.with(|b| b.set(false));
        show_state_quiet(result);
    });
}
