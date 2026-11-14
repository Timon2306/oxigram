//! The single local SQLite database: the Telegram session and the cache of
//! dialogs and messages share one connection.

pub mod chats;
pub mod session;

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

/// One connection shared by everything that touches the database. SQLite
/// serializes writers anyway, and one connection keeps one page cache.
pub type Db = Arc<Mutex<Connection>>;

const SCHEMA_VERSION: i64 = 14;

pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Db> {
    let mut conn = Connection::open(path)?;
    // WAL keeps readers and the writer from blocking each other; NORMAL is
    // durable across app crashes, which is what matters for the auth key.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    // Negative means KiB: cap the page cache at 2 MiB.
    conn.pragma_update(None, "cache_size", -2048)?;
    migrate(&mut conn)?;
    Ok(Arc::new(Mutex::new(conn)))
}

fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= SCHEMA_VERSION {
        return Ok(());
    }

    let tx = conn.transaction()?;
    if version < 1 {
        // Same layout as grammers' own SQLite session storage.
        tx.execute_batch(
            "CREATE TABLE dc_home (
                dc_id INTEGER NOT NULL,
                PRIMARY KEY (dc_id));
             CREATE TABLE dc_option (
                dc_id INTEGER NOT NULL,
                ipv4 TEXT NOT NULL,
                ipv6 TEXT NOT NULL,
                auth_key BLOB,
                PRIMARY KEY (dc_id));
             CREATE TABLE peer_info (
                peer_id INTEGER NOT NULL,
                hash INTEGER,
                subtype INTEGER,
                PRIMARY KEY (peer_id));
             CREATE TABLE update_state (
                pts INTEGER NOT NULL,
                qts INTEGER NOT NULL,
                date INTEGER NOT NULL,
                seq INTEGER NOT NULL);
             CREATE TABLE channel_state (
                peer_id INTEGER NOT NULL,
                pts INTEGER NOT NULL,
                PRIMARY KEY (peer_id));",
        )?;
    }
    if version < 14 {
        tx.execute_batch(chats::SCHEMA)?;
    }
    // PRAGMA arguments cannot be bound; the value is a constant, not user input.
    tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
    tx.commit()
}
