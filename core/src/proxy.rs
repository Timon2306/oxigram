//! A proxy for the Telegram connection, kept in a local file next to the API
//! keys. Without one, Telegram cannot be reached in countries that block it —
//! and the API keys can only be checked by connecting, so the proxy screen
//! comes before the keys.
//!
//! Only SOCKS5 works for now (grammers connects through it itself). HTTP
//! CONNECT and MTProto proxies are recognised but refused until supported.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::UiError;

const FILE_NAME: &str = "proxy.json";
/// A Telegram data centre (DC 2), to check that the proxy can reach Telegram.
const PROBE: (&str, u16) = ("149.154.167.51", 443);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    pub enabled: bool,
    /// "socks5" (works), "http" or "mtproto" (not yet).
    pub kind: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub username: String,
    /// Never logged: `Debug` hides it.
    #[serde(default)]
    pub password: String,
}

impl fmt::Debug for ProxyConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyConfig")
            .field("enabled", &self.enabled)
            .field("kind", &self.kind)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &if self.password.is_empty() { "" } else { "***" })
            .finish()
    }
}

impl ProxyConfig {
    /// Checks the fields before anything is saved or tried.
    pub fn validate(&self) -> Result<(), UiError> {
        if !self.enabled {
            return Ok(());
        }
        if self.kind != "socks5" {
            return Err(UiError::new("PROXY_UNSUPPORTED", "only SOCKS5 proxies are supported for now"));
        }
        let host = self.host.trim();
        if host.is_empty() || host.contains(char::is_whitespace) || host.contains('/') {
            return Err(UiError::new("PROXY_HOST_INVALID", "the proxy address is not valid"));
        }
        if self.port == 0 {
            return Err(UiError::new("PROXY_PORT_INVALID", "the proxy port is not valid"));
        }
        Ok(())
    }

    /// The URL grammers connects through, when the proxy is on and usable.
    pub fn url(&self) -> Option<String> {
        if !self.enabled || self.validate().is_err() {
            return None;
        }
        let host = self.host.trim();
        // An IPv6 address needs brackets inside a URL.
        let host = if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host.to_owned() };
        let auth = if self.username.is_empty() {
            String::new()
        } else {
            format!("{}:{}@", encode(&self.username), encode(&self.password))
        };
        Some(format!("socks5://{auth}{host}:{}", self.port))
    }
}

/// Percent-encodes a URL user-info part.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// The saved proxy; `None` when there is none or the file is unreadable.
pub fn load(dir: &Path) -> Option<ProxyConfig> {
    let raw = fs::read(path(dir)).ok()?;
    serde_json::from_slice(&raw).ok()
}

pub fn save(dir: &Path, config: &ProxyConfig) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let raw = serde_json::to_vec_pretty(config).map_err(io::Error::other)?;
    let path = path(dir);
    fs::write(&path, raw)?;
    crate::config::restrict_to_owner(&path)
}

/// Connects to Telegram through the proxy and returns how long it took.
/// Needs no API keys: it only proves the route works.
pub async fn check(config: &ProxyConfig) -> Result<Duration, UiError> {
    let mut config = config.clone();
    config.enabled = true;
    config.validate()?;
    let started = Instant::now();
    let host = config.host.trim();
    let proxy = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{}", config.port)
    } else {
        format!("{host}:{}", config.port)
    };
    let proxy = proxy.as_str();
    let attempt = async {
        if config.username.is_empty() {
            tokio_socks::tcp::Socks5Stream::connect(proxy, PROBE).await.map(drop)
        } else {
            tokio_socks::tcp::Socks5Stream::connect_with_password(
                proxy,
                PROBE,
                &config.username,
                &config.password,
            )
            .await
            .map(drop)
        }
    };
    match tokio::time::timeout(PROBE_TIMEOUT, attempt).await {
        Err(_) => Err(UiError::new("PROXY_TIMEOUT", "the proxy did not answer in time")),
        Ok(Err(tokio_socks::Error::PasswordAuthFailure(_)))
        | Ok(Err(tokio_socks::Error::AuthorizationRequired)) => {
            Err(UiError::new("PROXY_AUTH", "the proxy rejected the username or password"))
        }
        Ok(Err(tokio_socks::Error::ProxyServerUnreachable)) => {
            Err(UiError::new("PROXY_UNREACHABLE", "the proxy server is unreachable"))
        }
        Ok(Err(err)) => Err(UiError::new("PROXY_FAILED", err.to_string())),
        Ok(Ok(())) => Ok(started.elapsed()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn socks(host: &str, port: u16) -> ProxyConfig {
        ProxyConfig { enabled: true, kind: "socks5".into(), host: host.into(), port, ..Default::default() }
    }

    #[test]
    fn builds_socks5_urls() {
        assert_eq!(socks("127.0.0.1", 1080).url().as_deref(), Some("socks5://127.0.0.1:1080"));
        let mut with_auth = socks("proxy.example", 443);
        with_auth.username = "me".into();
        with_auth.password = "p@ss:w/rd".into();
        assert_eq!(with_auth.url().as_deref(), Some("socks5://me:p%40ss%3Aw%2Frd@proxy.example:443"));
        assert_eq!(socks("::1", 9050).url().as_deref(), Some("socks5://[::1]:9050"));
    }

    #[test]
    fn off_or_invalid_means_no_proxy() {
        let mut off = socks("127.0.0.1", 1080);
        off.enabled = false;
        assert!(off.url().is_none());
        assert!(socks("", 1080).validate().is_err());
        assert!(socks("host", 0).validate().is_err());
        let mut mtproto = socks("host", 443);
        mtproto.kind = "mtproto".into();
        assert!(mtproto.validate().is_err());
    }

    #[test]
    fn debug_hides_the_password() {
        let mut config = socks("host", 1);
        config.password = "secret".into();
        assert!(!format!("{config:?}").contains("secret"));
    }
}
