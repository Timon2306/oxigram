//! HTTP CONNECT proxies. grammers only speaks SOCKS5, so Oxigram runs a tiny
//! SOCKS5 server on 127.0.0.1 that opens each connection through the user's
//! HTTP proxy (`CONNECT host:port`) and then just relays the bytes. grammers
//! connects to `socks5://127.0.0.1:<port>` as to any other SOCKS5 proxy.

use std::io;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Mutex;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::proxy::ProxyConfig;

/// The running bridge: the proxy it goes through and its local port.
static BRIDGE: Mutex<Option<(ProxyConfig, u16)>> = Mutex::new(None);
/// Longest response header accepted from the proxy.
const MAX_HEADER: usize = 8 * 1024;

#[derive(Debug)]
pub(crate) enum ConnectError {
    Io(io::Error),
    /// 407: the proxy wants (other) credentials.
    Auth,
    /// Any other refusal, with the HTTP status.
    Status(u16),
    /// Not an HTTP answer.
    Garbled,
}

impl From<io::Error> for ConnectError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// The SOCKS5 URL for grammers, starting (or reusing) the bridge for this
/// proxy. Must run inside the Tokio runtime.
pub fn local_url(config: &ProxyConfig) -> Option<String> {
    let mut bridge = BRIDGE.lock().ok()?;
    if let Some((running, port)) = bridge.as_ref()
        && running == config
    {
        return Some(format!("socks5://127.0.0.1:{port}"));
    }
    let socket = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
    socket.set_nonblocking(true).ok()?;
    let port = socket.local_addr().ok()?.port();
    let listener = TcpListener::from_std(socket).ok()?;
    *bridge = Some((config.clone(), port));
    let upstream = config.clone();
    tokio::spawn(async move {
        loop {
            let Ok((client, _)) = listener.accept().await else { break };
            // A newer bridge replaced this one (the proxy was changed).
            let current = BRIDGE.lock().ok().and_then(|b| b.as_ref().map(|(_, p)| *p));
            if current != Some(port) {
                break;
            }
            let upstream = upstream.clone();
            tokio::spawn(async move {
                let _ = serve(client, &upstream).await;
            });
        }
    });
    Some(format!("socks5://127.0.0.1:{port}"))
}

/// One SOCKS5 client (grammers): handshake, CONNECT through the HTTP proxy,
/// then relay.
async fn serve(mut client: TcpStream, upstream: &ProxyConfig) -> io::Result<()> {
    let mut greeting = [0u8; 2];
    client.read_exact(&mut greeting).await?;
    if greeting[0] != 5 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not SOCKS5"));
    }
    let mut methods = vec![0u8; greeting[1] as usize];
    client.read_exact(&mut methods).await?;
    // No authentication between grammers and the bridge (loopback only).
    client.write_all(&[5, 0]).await?;

    let mut request = [0u8; 4];
    client.read_exact(&mut request).await?;
    let host = match request[3] {
        1 => {
            let mut ip = [0u8; 4];
            client.read_exact(&mut ip).await?;
            Ipv4Addr::from(ip).to_string()
        }
        4 => {
            let mut ip = [0u8; 16];
            client.read_exact(&mut ip).await?;
            format!("[{}]", Ipv6Addr::from(ip))
        }
        3 => {
            let mut len = [0u8; 1];
            client.read_exact(&mut len).await?;
            let mut name = vec![0u8; len[0] as usize];
            client.read_exact(&mut name).await?;
            String::from_utf8_lossy(&name).into_owned()
        }
        _ => {
            client.write_all(&[5, 8, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            return Ok(());
        }
    };
    let mut port = [0u8; 2];
    client.read_exact(&mut port).await?;
    let port = u16::from_be_bytes(port);
    // Only CONNECT is needed.
    if request[1] != 1 {
        client.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        return Ok(());
    }

    match connect(upstream, &host, port).await {
        Ok(mut tunnel) => {
            client.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            tokio::io::copy_bidirectional(&mut client, &mut tunnel).await?;
        }
        Err(_) => {
            // "Connection refused": grammers retries and reports the error.
            client.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        }
    }
    Ok(())
}

/// Opens a tunnel to `host:port` through the HTTP proxy.
pub(crate) async fn connect(proxy: &ProxyConfig, host: &str, port: u16) -> Result<TcpStream, ConnectError> {
    let proxy_host = proxy.host.trim();
    let address = if proxy_host.contains(':') && !proxy_host.starts_with('[') {
        format!("[{proxy_host}]:{}", proxy.port)
    } else {
        format!("{proxy_host}:{}", proxy.port)
    };
    let mut stream = TcpStream::connect(address).await?;
    let target = format!("{host}:{port}");
    let mut request = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if !proxy.username.is_empty() {
        let credentials = base64(format!("{}:{}", proxy.username, proxy.password).as_bytes());
        request.push_str(&format!("Proxy-Authorization: Basic {credentials}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await?;

    // Read the header byte by byte: nothing of the tunnel may be swallowed.
    let mut header = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= MAX_HEADER || stream.read(&mut byte).await? == 0 {
            return Err(ConnectError::Garbled);
        }
        header.push(byte[0]);
    }
    match status(&header) {
        Some(200..=299) => Ok(stream),
        Some(407) => Err(ConnectError::Auth),
        Some(code) => Err(ConnectError::Status(code)),
        None => Err(ConnectError::Garbled),
    }
}

/// The status code of an HTTP response header ("HTTP/1.1 200 Connection established").
fn status(header: &[u8]) -> Option<u16> {
    let line = header.split(|b| *b == b'\n').next()?;
    let line = std::str::from_utf8(line).ok()?;
    let mut parts = line.split_whitespace();
    if !parts.next()?.starts_with("HTTP/") {
        return None;
    }
    parts.next()?.parse().ok()
}

fn base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b""), "");
    }

    #[test]
    fn reads_the_status() {
        assert_eq!(status(b"HTTP/1.1 200 Connection established\r\n\r\n"), Some(200));
        assert_eq!(status(b"HTTP/1.0 407 Proxy Authentication Required\r\n"), Some(407));
        assert_eq!(status(b"SSH-2.0-OpenSSH\r\n"), None);
    }

    /// A fake HTTP proxy on loopback that accepts CONNECT and echoes; the
    /// bridge must carry a SOCKS5 client through it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn bridges_socks5_through_http_connect() {
        let fake = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let fake_port = fake.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = fake.accept().await.unwrap();
            let mut header = Vec::new();
            let mut byte = [0u8; 1];
            while !header.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut byte).await.unwrap();
                header.push(byte[0]);
            }
            let text = String::from_utf8_lossy(&header).into_owned();
            assert!(text.starts_with("CONNECT 149.154.167.51:443 HTTP/1.1"));
            assert!(text.contains("Proxy-Authorization: Basic dTpw"));
            s.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await.unwrap();
            let mut buf = [0u8; 5];
            s.read_exact(&mut buf).await.unwrap();
            s.write_all(&buf).await.unwrap();
        });
        let config = ProxyConfig {
            enabled: true,
            kind: "http".into(),
            host: "127.0.0.1".into(),
            port: fake_port,
            username: "u".into(),
            password: "p".into(),
        };
        let url = local_url(&config).unwrap();
        let local = url.trim_start_matches("socks5://").to_owned();
        let mut stream = tokio_socks::tcp::Socks5Stream::connect(local.as_str(), ("149.154.167.51", 443))
            .await
            .unwrap();
        stream.write_all(b"hello").await.unwrap();
        let mut back = [0u8; 5];
        stream.read_exact(&mut back).await.unwrap();
        assert_eq!(&back, b"hello");
    }
}
