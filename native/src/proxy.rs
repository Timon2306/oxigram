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
        window.set_proxy_status("Checking…".into());
        run(async move { oxigram_core::proxy::check(&config).await }, |result| {
            let Some(ui) = ui() else { return };
            ui.set_proxy_busy(false);
            match result {
                Ok(took) => {
                    ui.set_proxy_status_ok(true);
                    ui.set_proxy_status(format!("Working, Telegram is reachable ({} ms)", took.as_millis()).into());
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
                    crate::menus::toast("Proxy saved");
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

/// "Disabled" or "SOCKS5 · host:port" in the settings.
pub fn show_summary(ui: &AppWindow) {
    let config = tg().proxy();
    let text = if config.active() && config.is_websocket() {
        "Bypass blocking (WebSocket)".to_owned()
    } else if config.active() {
        let kind = if config.kind == "http" { "HTTP" } else { "SOCKS5" };
        format!("{kind} · {}:{}", config.host.trim(), config.port)
    } else {
        "Disabled".to_owned()
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
        "PROXY_UNSUPPORTED" => "MTProto proxies are not supported yet: choose SOCKS5 or HTTP.".to_owned(),
        "PROXY_HOST_INVALID" => "Check the proxy address.".to_owned(),
        "PROXY_PORT_INVALID" => "The port must be a number from 1 to 65535.".to_owned(),
        "PROXY_TIMEOUT" => "The proxy did not respond within 10 seconds.".to_owned(),
        "PROXY_FAILED" if fallback.contains("WebSocket") || fallback.contains("kws") => {
            format!("Telegram web addresses are unreachable: {fallback}")
        }
        "PROXY_UNREACHABLE" => "The proxy is unreachable: check the address and port.".to_owned(),
        "PROXY_AUTH" => "The proxy rejected the username or password.".to_owned(),
        "PROXY_FAILED" => format!("Could not connect through the proxy: {fallback}"),
        _ => fallback.to_owned(),
    }
}
