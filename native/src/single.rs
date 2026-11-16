//! One copy per profile. The first copy holds an exclusive lock on
//! `oxigram.single` in the config folder and listens on a loopback TCP port
//! (written to `oxigram.port`); a later launch fails to take the lock, sends
//! "show" to that port and exits, and the running copy brings its window to
//! the front, even from the tray. The lock dies with the process, so a crash
//! leaves nothing stale. The core's `oxigram.lock` stays as a safety net.
//! The folder is per user (and per OXIGRAM_HOME), so users never meet.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use slint::ComponentHandle;

use crate::{AppWindow, tray};

const LOCK_FILE: &str = "oxigram.single";
const PORT_FILE: &str = "oxigram.port";
/// First word of the port file and the message that wakes the window.
const MAGIC: &str = "oxigram-show";

/// Keeps the lock for the life of the process.
static HELD: Mutex<Option<(File, TcpListener)>> = Mutex::new(None);

pub enum Role {
    /// We are the only copy; call [`listen`] once the window exists.
    First,
    /// Another copy was told to show itself; exit.
    Second,
}

/// Call at the very start, before the database is opened. If the folder
/// cannot be used we carry on as the first copy (the core lock still guards
/// the database).
pub fn claim(dir: &Path) -> Role {
    let _ = std::fs::create_dir_all(dir);
    let Ok(lock) = OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(LOCK_FILE)) else {
        return Role::First;
    };
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            // The first copy may still be writing its port: wait a little.
            for _ in 0..30 {
                if ping(dir) {
                    return Role::Second;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            // Locked but not answering: leave it to the core lock to decide.
            return Role::First;
        }
        Err(_) => return Role::First,
    }
    let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) else {
        return Role::First;
    };
    if let Ok(addr) = listener.local_addr() {
        let _ = std::fs::write(dir.join(PORT_FILE), format!("{MAGIC} {}", addr.port()));
    }
    if let Ok(mut held) = HELD.lock() {
        *held = Some((lock, listener));
    }
    Role::First
}

/// Tells the running copy to show itself; true if it answered.
fn ping(dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(dir.join(PORT_FILE)) else { return false };
    let Some(port) = text.strip_prefix(MAGIC).and_then(|p| p.trim().parse::<u16>().ok()) else {
        return false;
    };
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(2)) else { return false };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    if stream.write_all(MAGIC.as_bytes()).is_err() {
        return false;
    }
    let mut ack = [0u8; 2];
    stream.read_exact(&mut ack).is_ok()
}

/// Starts the background listener for a first copy.
pub fn listen(ui: &AppWindow) {
    let Some(listener) = HELD.lock().ok().and_then(|h| h.as_ref().and_then(|(_, l)| l.try_clone().ok())) else {
        return;
    };
    let weak = ui.as_weak();
    let _ = std::thread::Builder::new().name("single".into()).stack_size(256 * 1024).spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0u8; MAGIC.len()];
            if stream.read_exact(&mut buf).is_ok() && buf == *MAGIC.as_bytes() {
                let _ = stream.write_all(b"ok");
                let _ = weak.upgrade_in_event_loop(|ui| tray::show_window(&ui));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_claim_finds_the_first() {
        let dir = std::env::temp_dir().join(format!("oxigram-single-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(matches!(claim(&dir), Role::First));
        // A separate handle in the same process sees the lock as taken.
        let lock = OpenOptions::new().write(true).open(dir.join(LOCK_FILE)).unwrap();
        assert!(matches!(lock.try_lock(), Err(TryLockError::WouldBlock)));
        // Answer one ping the way `listen` does.
        let listener = HELD.lock().unwrap().as_ref().unwrap().1.try_clone().unwrap();
        let t = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut b = [0u8; MAGIC.len()];
            s.read_exact(&mut b).unwrap();
            s.write_all(b"ok").unwrap();
            b
        });
        assert!(ping(&dir));
        assert_eq!(&t.join().unwrap(), MAGIC.as_bytes());
    }
}
