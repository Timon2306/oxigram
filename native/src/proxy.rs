//! The proxy window: reachable from the first screen (before the API keys,
//! which can only be checked by connecting) and from the settings.

use oxigram_core::proxy::ProxyConfig;
use slint::SharedString;

use crate::{AppWindow, format, login_step, run, tg, ui};

pub fn wire(window: &AppWindow) {
    window.on_open_proxy(|| {
        let Some(ui) = ui() else { return };
        let config = tg().proxy();
        ui.set_proxy_enabled(config.enabled || config.host.is_empty());
        ui.set_proxy_kind(if config.kind.is_empty() { "socks5".into() } else { config.kind.clone().into() });
        ui.set_proxy_host(config.host.clone().into());
        ui.set_proxy_port(if config.port == 0 { SharedString::new() } else { config.port.to_string().into() });
        ui.set_proxy_user(config.username.clone().into());
        ui.set_proxy_pass(config.password.clone().into());
        ui.set_proxy_status(SharedString::new());
        ui.set_proxy_status_ok(false);
        ui.set_proxy_busy(false);
        ui.set_proxy_open(true);
    });
    window.on_proxy_check(|| {
        let Some(window) = ui() else { return };
        let mut config = from_ui(&window);
        config.enabled = true;
        window.set_proxy_busy(true);
        window.set_proxy_status_ok(false);
        window.set_proxy_status("Проверка…".into());
        run(async move { oxigram_core::proxy::check(&config).await }, |result| {
            let Some(ui) = ui() else { return };
            ui.set_proxy_busy(false);
            match result {
                Ok(took) => {
                    ui.set_proxy_status_ok(true);
                    ui.set_proxy_status(format!("Работает, Telegram доступен ({} мс)", took.as_millis()).into());
                }
                Err(err) => ui.set_proxy_status(proxy_error(&err.code, &format::error(&err)).into()),
            }
        });
    });
    window.on_proxy_save(|| {
        let Some(window) = ui() else { return };
        let config = from_ui(&window);
        if let Err(err) = config.validate() {
            window.set_proxy_status_ok(false);
            window.set_proxy_status(proxy_error(&err.code, &format::error(&err)).into());
            return;
        }
        window.set_proxy_busy(true);
        run(async move { tg().set_proxy(config).await }, |result| {
            let Some(ui) = ui() else { return };
            ui.set_proxy_busy(false);
            match result {
                Ok(()) => {
                    ui.set_proxy_open(false);
                    show_summary(&ui);
                    crate::menus::toast("Прокси сохранён");
                    // Reconnect through the new route (also refreshes the login screen).
                    login_step(async { tg().state().await });
                }
                Err(err) => {
                    ui.set_proxy_status_ok(false);
                    ui.set_proxy_status(format::error(&err).into());
                }
            }
        });
    });
    show_summary(window);
}

/// "Выключен" or "SOCKS5 · host:port" in the settings.
pub fn show_summary(ui: &AppWindow) {
    let config = tg().proxy();
    let text = if config.active() {
        let kind = if config.kind == "http" { "HTTP" } else { "SOCKS5" };
        format!("{kind} · {}:{}", config.host.trim(), config.port)
    } else {
        "Выключен".to_owned()
    };
    ui.set_proxy_summary(text.into());
}

fn from_ui(ui: &AppWindow) -> ProxyConfig {
    ProxyConfig {
        enabled: ui.get_proxy_enabled(),
        kind: ui.get_proxy_kind().to_string(),
        host: ui.get_proxy_host().trim().to_owned(),
        port: ui.get_proxy_port().trim().parse().unwrap_or(0),
        username: ui.get_proxy_user().trim().to_owned(),
        password: ui.get_proxy_pass().to_string(),
    }
}

fn proxy_error(code: &str, fallback: &str) -> String {
    match code {
        "PROXY_UNSUPPORTED" => "MTProto-прокси пока не поддерживается: выберите SOCKS5 или HTTP.".to_owned(),
        "PROXY_HOST_INVALID" => "Проверьте адрес прокси.".to_owned(),
        "PROXY_PORT_INVALID" => "Порт — число от 1 до 65535.".to_owned(),
        "PROXY_TIMEOUT" => "Прокси не ответил за 10 секунд.".to_owned(),
        "PROXY_UNREACHABLE" => "Прокси недоступен: проверьте адрес и порт.".to_owned(),
        "PROXY_AUTH" => "Прокси не принял логин или пароль.".to_owned(),
        "PROXY_FAILED" => format!("Не удалось подключиться через прокси: {fallback}"),
        _ => fallback.to_owned(),
    }
}
