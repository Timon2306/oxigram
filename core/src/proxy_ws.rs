//! "Обход блокировок (WebSocket)": Telegram without a proxy server of one's
//! own, through the WebSocket endpoints that Telegram Web uses
//! (`wss://kws{dc}.web.telegram.org/apiws`). Where plain MTProto to the data
//! centres is blocked, these HTTPS addresses often still work.
//!
//! grammers only speaks SOCKS5, so, as for HTTP proxies (`proxy_http`), a
//! tiny SOCKS5 server on 127.0.0.1 takes each connection. grammers then uses
//! the obfuscated transport (the 64-byte header and AES-CTR stream that
//! Telegram Web also sends over WebSocket); the bridge learns the data centre
//! from the address grammers asked for, writes its number into the header,
//! and relays the bytes as binary WebSocket messages.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::{self, pki_types::ServerName};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

/// Port of the running bridge.
static BRIDGE: Mutex<Option<u16>> = Mutex::new(None);
/// The WebSocket front of Telegram's web clients, used when the name does
/// not resolve (DNS blocking); TLS still checks the certificate for the name.
const FALLBACK_IP: Ipv4Addr = Ipv4Addr::new(149, 154, 167, 220);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Bytes read from grammers at once.
const CHUNK: usize = 64 * 1024;

/// The SOCKS5 URL for grammers, starting the bridge on first use. Must run
/// inside the Tokio runtime.
pub fn local_url() -> Option<String> {
    let mut bridge = BRIDGE.lock().ok()?;
    if let Some(port) = *bridge {
        return Some(format!("socks5://127.0.0.1:{port}"));
    }
    let socket = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
    socket.set_nonblocking(true).ok()?;
    let port = socket.local_addr().ok()?.port();
    let listener = TcpListener::from_std(socket).ok()?;
    *bridge = Some(port);
    tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            tokio::spawn(async move {
                let _ = serve(client).await;
            });
        }
        if let Ok(mut bridge) = BRIDGE.lock() {
            *bridge = None;
        }
    });
    Some(format!("socks5://127.0.0.1:{port}"))
}

/// One SOCKS5 client (grammers): handshake, open the WebSocket of the data
/// centre it asked for, then relay.
async fn serve(mut client: TcpStream) -> io::Result<()> {
    client.set_nodelay(true)?;
    let mut greeting = [0u8; 2];
    client.read_exact(&mut greeting).await?;
    if greeting[0] != 5 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not SOCKS5"));
    }
    let mut methods = vec![0u8; greeting[1] as usize];
    client.read_exact(&mut methods).await?;
    client.write_all(&[5, 0]).await?;

    let mut request = [0u8; 4];
    client.read_exact(&mut request).await?;
    let ip: Option<IpAddr> = match request[3] {
        1 => {
            let mut ip = [0u8; 4];
            client.read_exact(&mut ip).await?;
            Some(Ipv4Addr::from(ip).into())
        }
        4 => {
            let mut ip = [0u8; 16];
            client.read_exact(&mut ip).await?;
            Some(Ipv6Addr::from(ip).into())
        }
        3 => {
            let mut len = [0u8; 1];
            client.read_exact(&mut len).await?;
            let mut name = vec![0u8; len[0] as usize];
            client.read_exact(&mut name).await?;
            None
        }
        _ => {
            client.write_all(&[5, 8, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            return Ok(());
        }
    };
    let mut port = [0u8; 2];
    client.read_exact(&mut port).await?;
    if request[1] != 1 {
        client.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        return Ok(());
    }
    let dc = ip.and_then(dc_of).unwrap_or(2);

    let ws = match tokio::time::timeout(CONNECT_TIMEOUT, open(dc)).await {
        Ok(Ok(ws)) => ws,
        _ => {
            // "Connection refused": grammers retries and reports the error.
            client.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            return Ok(());
        }
    };
    client.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
    relay(client, ws, dc).await
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;

/// Relays until either side closes. The first 64 bytes from grammers are
/// the obfuscation header, which gets the data centre's number.
async fn relay(client: TcpStream, ws: Ws, dc: i16) -> io::Result<()> {
    let (mut client_read, mut client_write) = client.into_split();
    let (mut ws_write, mut ws_read) = ws.split();

    let upstream = async {
        let mut buf = vec![0u8; CHUNK];
        let mut filled = 0;
        // The header and the first packet usually come in one write.
        while filled < 64 {
            let n = client_read.read(&mut buf[filled..]).await?;
            if n == 0 {
                return Ok::<(), io::Error>(());
            }
            filled += n;
        }
        let head: &mut [u8; 64] = (&mut buf[..64]).try_into().expect("64 bytes");
        set_dc(head, dc);
        ws_write.send(Message::binary(buf[..filled].to_vec())).await.map_err(io::Error::other)?;
        loop {
            let n = client_read.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            ws_write.send(Message::binary(buf[..n].to_vec())).await.map_err(io::Error::other)?;
        }
        let _ = ws_write.close().await;
        Ok(())
    };
    let downstream = async {
        while let Some(message) = ws_read.next().await {
            match message.map_err(io::Error::other)? {
                Message::Binary(data) => client_write.write_all(&data).await?,
                Message::Close(_) => break,
                // Pings are answered by tungstenite with the next write.
                _ => {}
            }
        }
        client_write.shutdown().await
    };
    // Whichever side ends first ends the connection.
    tokio::select! {
        result = upstream => result,
        result = downstream => result,
    }
}

/// The host name of a data centre's WebSocket.
fn host(dc: i16) -> String {
    format!("kws{}.web.telegram.org", dc.unsigned_abs())
}

/// How long the front IP gets before the name is tried instead.
const IP_FIRST: Duration = Duration::from_secs(3);

/// TLS, then the WebSocket handshake (subprotocol "binary", as Telegram Web).
///
/// Telegram's front IP is tried first and the name only second: in the very
/// networks this bypass is for, DNS for `web.telegram.org` hangs for ~20 s
/// (longer than a connection is given) or answers with a blocked address,
/// while the IP of the same front works. The name is still the one the
/// certificate is checked against, so an answer by IP is as trustworthy as
/// by name.
async fn open(dc: i16) -> io::Result<Ws> {
    let host = host(dc);
    let by_ip = async {
        let tcp = TcpStream::connect((FALLBACK_IP, 443)).await?;
        handshake(&host, tcp).await
    };
    match tokio::time::timeout(IP_FIRST, by_ip).await {
        Ok(Ok(ws)) => return Ok(ws),
        // Refused, blocked or slow: the name may still answer.
        _ => {}
    }
    let tcp = TcpStream::connect((host.as_str(), 443)).await?;
    handshake(&host, tcp).await
}

/// The TLS and WebSocket part, once a socket to the front exists.
async fn handshake(host: &str, tcp: TcpStream) -> io::Result<Ws> {
    tcp.set_nodelay(true)?;
    let name = ServerName::try_from(host.to_owned()).map_err(io::Error::other)?;
    let tls = TlsConnector::from(tls_config()).connect(name, tcp).await?;
    let mut request = format!("wss://{host}/apiws").into_client_request().map_err(io::Error::other)?;
    request.headers_mut().insert("Sec-WebSocket-Protocol", HeaderValue::from_static("binary"));
    let (ws, _) = tokio_tungstenite::client_async(request, tls).await.map_err(io::Error::other)?;
    Ok(ws)
}

/// rustls with the ring provider and the Mozilla root certificates.
pub(crate) fn tls_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
            let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .expect("ring supports the default versions")
                .with_root_certificates(roots)
                .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}

/// Which data centre an address belongs to (Telegram's published ranges).
fn dc_of(ip: IpAddr) -> Option<i16> {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, d] = ip.octets();
            match (a, b, c) {
                (149, 154, 175) => Some(if d < 100 || d >= 200 { 1 } else { 3 }),
                (149, 154, 167) => Some(if (90..150).contains(&d) { 4 } else { 2 }),
                (149, 154, 164..=166) => Some(4),
                (91, 108, 56..=59) => Some(5),
                (91, 108, 4..=7) => Some(4),
                (91, 108, 8..=11) => Some(3),
                (91, 108, 12..=15) => Some(1),
                (91, 108, 16..=19) => Some(2),
                (95, 161, 76) => Some(2),
                _ => None,
            }
        }
        // 2001:b28:f23d:f001::a is DC 1 … the fourth group ends in the number.
        IpAddr::V6(ip) => {
            let group = ip.segments()[3];
            let dc = (group & 0x000f) as i16;
            ((group & 0xfff0) == 0xf000 && (1..=5).contains(&dc)).then_some(dc)
        }
    }
}

/// Writes the data centre into bytes 60..62 of the obfuscation header.
/// Bytes 56..64 are sent encrypted with the stream's own key (taken from
/// bytes 8..56), so they are decrypted, changed and encrypted again; the
/// rest of the stream is unaffected.
fn set_dc(head: &mut [u8; 64], dc: i16) {
    let mut keystream = [0u8; 64];
    grammers_crypto::ObfuscatedCipher::new(head).encrypt(&mut keystream);
    let mut plain = [0u8; 8];
    for i in 0..8 {
        plain[i] = head[56 + i] ^ keystream[56 + i];
    }
    plain[4..6].copy_from_slice(&dc.to_le_bytes());
    for i in 0..8 {
        head[56 + i] = plain[i] ^ keystream[56 + i];
    }
}

/// Checks that the WebSocket of DC 2 opens: the route works.
pub(crate) async fn probe() -> io::Result<()> {
    let mut ws = open(2).await?;
    let _ = ws.close(None).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_centres_by_address() {
        let v4 = |s: &str| s.parse::<IpAddr>().unwrap();
        assert_eq!(dc_of(v4("149.154.175.53")), Some(1));
        assert_eq!(dc_of(v4("149.154.167.51")), Some(2));
        assert_eq!(dc_of(v4("149.154.175.100")), Some(3));
        assert_eq!(dc_of(v4("149.154.167.91")), Some(4));
        assert_eq!(dc_of(v4("91.108.56.130")), Some(5));
        assert_eq!(dc_of(v4("2001:b28:f23d:f001::a")), Some(1));
        assert_eq!(dc_of(v4("2001:67c:4e8:f004::a")), Some(4));
        assert_eq!(dc_of(v4("8.8.8.8")), None);
    }

    #[test]
    fn dc_goes_into_the_encrypted_header() {
        let mut head = [0u8; 64];
        for (i, b) in head.iter_mut().enumerate() {
            *b = (i * 7 + 3) as u8;
        }
        let original = head;
        set_dc(&mut head, 4);
        // The key part is untouched.
        assert_eq!(head[..56], original[..56]);
        // Decrypting shows the number at 60..62, and the tag at 56..60 kept.
        let mut keystream = [0u8; 64];
        grammers_crypto::ObfuscatedCipher::new(&head).encrypt(&mut keystream);
        let plain: Vec<u8> = (56..64).map(|i| head[i] ^ keystream[i]).collect();
        let before: Vec<u8> = (56..64).map(|i| original[i] ^ keystream[i]).collect();
        assert_eq!(&plain[4..6], &4i16.to_le_bytes());
        assert_eq!(plain[..4], before[..4]);
        assert_eq!(plain[6..], before[6..]);
    }

    #[test]
    fn hosts() {
        assert_eq!(host(2), "kws2.web.telegram.org");
        assert_eq!(host(-4), "kws4.web.telegram.org");
    }

    /// Opens the real WebSocket of DC 2 (needs the network).
    #[tokio::test]
    #[ignore]
    async fn opens_the_real_websocket() {
        probe().await.expect("kws2.web.telegram.org answers");
    }

    /// The whole route: grammers' obfuscated transport through the local
    /// bridge and the WebSocket to DC 2, far enough to create an
    /// authorization key (no API keys, no account involved).
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn mtproto_through_the_bridge() {
        use grammers_client::sender::{ServerAddr, connect};
        use grammers_mtproto::transport::{Intermediate, Obfuscated};
        let proxy = local_url().expect("bridge starts");
        let addr = ServerAddr::Proxied { address: "149.154.167.51:443".parse().unwrap(), proxy };
        let sender = tokio::time::timeout(Duration::from_secs(30), connect(Obfuscated::new(Intermediate::new()), addr))
            .await
            .expect("in time");
        assert!(sender.is_ok(), "auth key over WebSocket: {:?}", sender.err());
    }
}
