//! Update check: one request to GitHub's "latest release" (published, not
//! drafts or pre-releases), and, when asked, the download of this system's
//! installer, checked against the release's SHA256SUMS.txt.
//!
//! Nothing else is sent: no telemetry, no identifiers, the User-Agent is
//! "Oxigram/<version>". The request goes through the user's proxy when one
//! is set (SOCKS5 or HTTP); the WebSocket bypass only reaches Telegram, so
//! then GitHub is asked directly.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::pki_types::ServerName;

use crate::error::UiError;
use crate::proxy::ProxyConfig;

const LATEST: &str = "https://api.github.com/repos/Timon2306/oxigram/releases/latest";
const SUMS: &str = "SHA256SUMS.txt";
const TIMEOUT: Duration = Duration::from_secs(20);
/// GitHub sends assets through one or two redirects.
const MAX_REDIRECTS: usize = 5;
/// Largest answer kept in memory (the release JSON, the sums file).
const MAX_SMALL: usize = 4 * 1024 * 1024;

/// A newer published release.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    /// "0.3.1"
    pub version: String,
    /// The release page ("Что нового").
    pub page: String,
    /// This system's installer, when the release has one.
    pub asset: Option<Asset>,
    /// The URL of SHA256SUMS.txt, when the release has it.
    pub sums: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// "v0.3.1" or "0.3.1" as numbers; `None` for anything else.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim().trim_start_matches(['v', 'V']);
    // "0.3.1-beta" is not offered: pre-release suffixes are not parsed.
    let mut parts = text.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Whether `latest` is newer than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// The installer for this system among the release's files:
/// oxigram-{windows-x64,windows-x86,windows-arm64}-…-setup.exe,
/// oxigram-macos-{apple-silicon,intel}-….dmg, oxigram-linux-x64-….AppImage
/// (or .deb when not running as an AppImage).
fn pick_asset<'a>(names: impl Iterator<Item = &'a str>, prefix: &str, suffix: &str) -> Option<&'a str> {
    names.into_iter().find(|n| n.starts_with(prefix) && n.ends_with(suffix))
}

/// (prefix, suffix) of this system's installer name.
pub fn platform() -> Option<(&'static str, &'static str)> {
    if cfg!(target_os = "windows") {
        let prefix = if cfg!(target_arch = "x86_64") {
            "oxigram-windows-x64-"
        } else if cfg!(target_arch = "x86") {
            "oxigram-windows-x86-"
        } else if cfg!(target_arch = "aarch64") {
            "oxigram-windows-arm64-"
        } else {
            return None;
        };
        Some((prefix, "-setup.exe"))
    } else if cfg!(target_os = "macos") {
        Some((if cfg!(target_arch = "aarch64") { "oxigram-macos-apple-silicon-" } else { "oxigram-macos-intel-" }, ".dmg"))
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        let appimage = std::env::var_os("APPIMAGE").is_some();
        Some(("oxigram-linux-x64-", if appimage { ".AppImage" } else { ".deb" }))
    } else {
        None
    }
}

fn release_from(raw: RawRelease, platform: Option<(&str, &str)>) -> Option<Release> {
    if raw.draft || raw.prerelease {
        return None;
    }
    let version = parse_version(&raw.tag_name)?;
    let asset = platform.and_then(|(prefix, suffix)| {
        let name = pick_asset(raw.assets.iter().map(|a| a.name.as_str()), prefix, suffix)?;
        let found = raw.assets.iter().find(|a| a.name == name)?;
        Some(Asset { name: found.name.clone(), url: found.browser_download_url.clone(), size: found.size })
    });
    let sums = raw.assets.iter().find(|a| a.name == SUMS).map(|a| a.browser_download_url.clone());
    Some(Release {
        version: format!("{}.{}.{}", version.0, version.1, version.2),
        page: raw.html_url,
        asset,
        sums,
    })
}

/// The latest published release when it is newer than `current`.
pub async fn check(proxy: &ProxyConfig, current: &str) -> Result<Option<Release>, UiError> {
    let body = tokio::time::timeout(TIMEOUT, get_small(proxy, LATEST))
        .await
        .map_err(|_| UiError::new("UPDATE_TIMEOUT", "GitHub did not answer in time"))??;
    let raw: RawRelease = serde_json::from_slice(&body).map_err(|e| UiError::new("UPDATE_FAILED", e.to_string()))?;
    Ok(release_from(raw, platform()).filter(|r| is_newer(&r.version, current)))
}

/// Downloads the release's installer into `dir`, reporting bytes so far,
/// and checks it against SHA256SUMS.txt. A file that does not match (or a
/// release without the sums) is removed and refused.
pub async fn download(
    proxy: &ProxyConfig,
    release: &Release,
    dir: &Path,
    progress: impl Fn(u64),
) -> Result<PathBuf, UiError> {
    let asset = release.asset.as_ref().ok_or_else(|| UiError::new("UPDATE_NO_ASSET", "no installer for this system"))?;
    let sums_url = release.sums.as_ref().ok_or_else(|| UiError::new("UPDATE_NO_SUMS", "the release has no SHA256SUMS.txt"))?;
    let sums = String::from_utf8_lossy(&get_small(proxy, sums_url).await?).into_owned();
    let expected = expected_sum(&sums, &asset.name)
        .ok_or_else(|| UiError::new("UPDATE_NO_SUMS", "no checksum for this file"))?;

    std::fs::create_dir_all(dir)?;
    // Only the file name of the asset, never a path from the server.
    let name = Path::new(&asset.name).file_name().ok_or_else(|| UiError::new("UPDATE_FAILED", "bad file name"))?;
    let path = dir.join(name);
    let partial = dir.join(format!("{}.part", name.to_string_lossy()));
    let result = async {
        let digest = get_to(proxy, &asset.url, &partial, &progress).await?;
        if !hex(&digest).eq_ignore_ascii_case(&expected) {
            return Err(UiError::new("UPDATE_CHECKSUM", "the downloaded file does not match its checksum"));
        }
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        std::fs::rename(&partial, &path)?;
        Ok(path.clone())
    }
    .await;
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result
}

/// The checksum of `name` in a `sha256sum` style file ("<hex>  <name>").
fn expected_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let sum = parts.next()?;
        let file = parts.next()?.trim_start_matches('*');
        (file == name && sum.len() == 64 && sum.chars().all(|c| c.is_ascii_hexdigit())).then(|| sum.to_owned())
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---- A small HTTPS GET -------------------------------------------------------

trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

/// TCP to `host:443`, through the user's proxy when one is set.
async fn tcp(proxy: &ProxyConfig, host: &str) -> io::Result<TcpStream> {
    if proxy.active() && proxy.kind == "http" {
        return crate::proxy_http::connect(proxy, host, 443).await.map_err(|e| io::Error::other(format!("{e:?}")));
    }
    if proxy.active() && proxy.kind == "socks5" {
        let address = {
            let h = proxy.host.trim();
            if h.contains(':') && !h.starts_with('[') { format!("[{h}]:{}", proxy.port) } else { format!("{h}:{}", proxy.port) }
        };
        let stream = if proxy.username.is_empty() {
            tokio_socks::tcp::Socks5Stream::connect(address.as_str(), (host, 443)).await
        } else {
            tokio_socks::tcp::Socks5Stream::connect_with_password(
                address.as_str(),
                (host, 443),
                &proxy.username,
                &proxy.password,
            )
            .await
        };
        return stream.map(|s| s.into_inner()).map_err(io::Error::other);
    }
    TcpStream::connect((host, 443)).await
}

/// Splits "https://host/path" into (host, "/path").
fn split_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if host.is_empty() || host.contains(['@', ' ']) {
        return None;
    }
    Some((host.to_owned(), path.to_owned()))
}

/// Opens `url` and returns the stream positioned at the body of a 200
/// answer, following redirects, plus the body's length when known and
/// whether it is chunked.
async fn open(proxy: &ProxyConfig, url: &str) -> Result<(Box<dyn Stream>, Option<u64>, bool), UiError> {
    let mut url = url.to_owned();
    for _ in 0..=MAX_REDIRECTS {
        let (host, path) = split_url(&url).ok_or_else(|| UiError::new("UPDATE_FAILED", "not an https address"))?;
        let tcp = tcp(proxy, &host).await?;
        let name = ServerName::try_from(host.clone()).map_err(|e| UiError::new("UPDATE_FAILED", e.to_string()))?;
        let mut tls = TlsConnector::from(crate::proxy_ws::tls_config()).connect(name, tcp).await?;
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: Oxigram/{}\r\nAccept: application/vnd.github+json, application/octet-stream, */*\r\nConnection: close\r\n\r\n",
            env!("CARGO_PKG_VERSION")
        );
        tls.write_all(request.as_bytes()).await?;
        let header = read_header(&mut tls).await?;
        let status = header
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u16>().ok())
            .ok_or_else(|| UiError::new("UPDATE_FAILED", "not an HTTP answer"))?;
        let field = |name: &str| {
            header.lines().skip(1).find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.trim().eq_ignore_ascii_case(name).then(|| v.trim().to_owned())
            })
        };
        match status {
            200 => {
                let length = field("content-length").and_then(|v| v.parse().ok());
                let chunked = field("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked"));
                return Ok((Box::new(tls), length, chunked));
            }
            301 | 302 | 303 | 307 | 308 => {
                url = field("location").ok_or_else(|| UiError::new("UPDATE_FAILED", "redirect without a location"))?;
            }
            404 => return Err(UiError::new("UPDATE_NONE", "no published release")),
            code => return Err(UiError::new("UPDATE_FAILED", format!("GitHub answered HTTP {code}"))),
        }
    }
    Err(UiError::new("UPDATE_FAILED", "too many redirects"))
}

async fn read_header(stream: &mut (impl AsyncRead + Unpin)) -> Result<String, UiError> {
    let mut header = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() > 64 * 1024 || stream.read(&mut byte).await? == 0 {
            return Err(UiError::new("UPDATE_FAILED", "the answer ended early"));
        }
        header.push(byte[0]);
    }
    Ok(String::from_utf8_lossy(&header).into_owned())
}

/// Reads the body, de-chunked, handing each piece to `sink`.
async fn read_body(
    mut stream: Box<dyn Stream>,
    length: Option<u64>,
    chunked: bool,
    mut sink: impl FnMut(&[u8]) -> Result<(), UiError>,
) -> Result<(), UiError> {
    let mut buf = vec![0u8; 64 * 1024];
    if chunked {
        loop {
            let mut line = Vec::new();
            let mut byte = [0u8; 1];
            while !line.ends_with(b"\r\n") {
                if stream.read(&mut byte).await? == 0 {
                    return Err(UiError::new("UPDATE_FAILED", "the answer ended early"));
                }
                line.push(byte[0]);
            }
            let size_text = String::from_utf8_lossy(&line);
            let size = u64::from_str_radix(size_text.trim().split(';').next().unwrap_or(""), 16)
                .map_err(|_| UiError::new("UPDATE_FAILED", "bad chunk"))?;
            if size == 0 {
                return Ok(());
            }
            let mut left = size;
            while left > 0 {
                let want = left.min(buf.len() as u64) as usize;
                let n = stream.read(&mut buf[..want]).await?;
                if n == 0 {
                    return Err(UiError::new("UPDATE_FAILED", "the answer ended early"));
                }
                sink(&buf[..n])?;
                left -= n as u64;
            }
            let mut crlf = [0u8; 2];
            stream.read_exact(&mut crlf).await?;
        }
    }
    let mut got = 0u64;
    loop {
        let n = match stream.read(&mut buf).await {
            Ok(n) => n,
            // Servers often close TLS without a close_notify after the body.
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof && length.is_none_or(|l| got >= l) => 0,
            Err(e) => return Err(e.into()),
        };
        if n == 0 {
            break;
        }
        sink(&buf[..n])?;
        got += n as u64;
        if length.is_some_and(|l| got >= l) {
            break;
        }
    }
    if length.is_some_and(|l| got < l) {
        return Err(UiError::new("UPDATE_FAILED", "the download ended early"));
    }
    Ok(())
}

/// A small answer (JSON, the sums file) in memory.
async fn get_small(proxy: &ProxyConfig, url: &str) -> Result<Vec<u8>, UiError> {
    let (stream, length, chunked) = open(proxy, url).await?;
    let mut body = Vec::new();
    read_body(stream, length, chunked, |piece| {
        if body.len() + piece.len() > MAX_SMALL {
            return Err(UiError::new("UPDATE_FAILED", "the answer is too large"));
        }
        body.extend_from_slice(piece);
        Ok(())
    })
    .await?;
    Ok(body)
}

/// A file to disk; returns its SHA-256.
async fn get_to(proxy: &ProxyConfig, url: &str, path: &Path, progress: &impl Fn(u64)) -> Result<Vec<u8>, UiError> {
    use std::io::Write;
    let (stream, length, chunked) = open(proxy, url).await?;
    let mut file = std::fs::File::create(path)?;
    let mut hasher = ring::digest::Context::new(&ring::digest::SHA256);
    let mut total = 0u64;
    read_body(stream, length, chunked, |piece| {
        file.write_all(piece)?;
        hasher.update(piece);
        total += piece.len() as u64;
        progress(total);
        Ok(())
    })
    .await?;
    file.flush()?;
    Ok(hasher.finish().as_ref().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("v0.3.1", "0.3.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("v0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
        assert!(!is_newer("0.4.0-beta", "0.3.0"));
        assert_eq!(parse_version("v1.2"), Some((1, 2, 0)));
    }

    #[test]
    fn picks_the_installer_and_sums() {
        let raw = RawRelease {
            tag_name: "v0.3.1".into(),
            html_url: "https://github.com/x/releases/tag/v0.3.1".into(),
            draft: false,
            prerelease: false,
            assets: vec![
                RawAsset { name: "oxigram-windows-x86-0.3.1-setup.exe".into(), browser_download_url: "https://a/x86".into(), size: 1 },
                RawAsset { name: "oxigram-windows-x64-0.3.1-setup.exe".into(), browser_download_url: "https://a/x64".into(), size: 2 },
                RawAsset { name: "SHA256SUMS.txt".into(), browser_download_url: "https://a/sums".into(), size: 3 },
            ],
        };
        let release = release_from(raw, Some(("oxigram-windows-x64-", "-setup.exe"))).unwrap();
        assert_eq!(release.version, "0.3.1");
        assert_eq!(release.asset.unwrap().url, "https://a/x64");
        assert_eq!(release.sums.as_deref(), Some("https://a/sums"));
    }

    #[test]
    fn prereleases_are_not_offered() {
        let raw = RawRelease { tag_name: "v9.0.0".into(), html_url: String::new(), draft: false, prerelease: true, assets: vec![] };
        assert_eq!(release_from(raw, None), None);
    }

    #[test]
    fn sums_file() {
        let sums = format!("{}  oxigram-a.exe\n{} *oxigram-b.dmg\n", "a".repeat(64), "B".repeat(64));
        assert_eq!(expected_sum(&sums, "oxigram-a.exe"), Some("a".repeat(64)));
        assert_eq!(expected_sum(&sums, "oxigram-b.dmg"), Some("B".repeat(64)));
        assert_eq!(expected_sum(&sums, "other"), None);
    }

    #[test]
    fn urls() {
        assert_eq!(split_url("https://api.github.com/repos/a"), Some(("api.github.com".into(), "/repos/a".into())));
        assert_eq!(split_url("http://x/y"), None);
        assert_eq!(split_url("https://user@x/y"), None);
    }

    /// Asks GitHub for real (needs the network).
    #[tokio::test]
    #[ignore]
    async fn asks_github() {
        let result = check(&ProxyConfig::default(), "0.0.1").await;
        assert!(result.is_ok(), "{result:?}");
    }

    /// Downloads this system's installer of the latest release through the
    /// redirects and checks its size (needs the network).
    #[tokio::test]
    #[ignore]
    async fn downloads_an_asset() {
        let release = check(&ProxyConfig::default(), "0.0.1").await.unwrap().expect("a release");
        let asset = release.asset.expect("an installer for this system");
        let path = std::env::temp_dir().join("oxigram-update-test.bin");
        let digest = get_to(&ProxyConfig::default(), &asset.url, &path, &|_| {}).await.unwrap();
        let size = std::fs::metadata(&path).unwrap().len();
        let _ = std::fs::remove_file(&path);
        assert_eq!(size, asset.size);
        assert_eq!(digest.len(), 32);
    }
}
