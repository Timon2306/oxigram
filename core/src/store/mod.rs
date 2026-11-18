//! The single local SQLite database: the Telegram session and the cache of
//! dialogs and messages share one connection.

pub mod chats;
pub mod crypt;
pub mod session;

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

/// One connection shared by everything that touches the database. SQLite
/// serializes writers anyway, and one connection keeps one page cache.
pub type Db = Arc<Mutex<Connection>>;

const SCHEMA_VERSION: i64 = 14;

pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Db> {
    let path = path.as_ref();
    let mut conn = match connect(path) {
        Ok(conn) if is_healthy(&conn) => conn,
        // A damaged file (a crash or a disk hiccup mid-write) would break
        // every cached list and history for good. The cache can always be
        // fetched again; the sign-in is kept whenever its rows still read.
        broken => {
            let saved = broken.map(|conn| salvage(&conn)).unwrap_or_default();
            set_aside(path);
            let mut conn = connect(path)?;
            migrate(&mut conn)?;
            restore(&mut conn, &saved);
            log(path, &format!("rebuilt; healthy now: {}", is_healthy(&conn)));
            conn
        }
    };
    migrate(&mut conn)?;
    Ok(Arc::new(Mutex::new(conn)))
}

/// Writes a database image (the 0.3.1 development format sealed the whole
/// file) back to a file on disk: a serialized image is the file itself.
pub fn write_file(image: Vec<u8>, path: &Path) -> std::io::Result<()> {
    for stale in ["-wal", "-shm"] {
        let mut name = path.as_os_str().to_owned();
        name.push(stale);
        let _ = std::fs::remove_file(std::path::PathBuf::from(name));
    }
    std::fs::write(path, image)
}

fn connect(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    // WAL keeps readers and the writer from blocking each other; NORMAL is
    // durable across app crashes, which is what matters for the auth key.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    // Negative means KiB: cap the page cache at 2 MiB.
    conn.pragma_update(None, "cache_size", -2048)?;
    // enc() / dec() exist on every connection; without a passcode they keep
    // values as they are.
    crypt::use_key(&conn, None)?;
    Ok(conn)
}

/// `quick_check` reads every page once: a few milliseconds for this database.
fn is_healthy(conn: &Connection) -> bool {
    conn.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .is_ok_and(|result| result == "ok")
}

/// The session's tables, in the order they are restored.
const SESSION_TABLES: [&str; 5] = ["dc_home", "dc_option", "peer_info", "update_state", "channel_state"];

type Rows = Vec<(&'static str, Vec<Vec<rusqlite::types::Value>>)>;

/// Reads whatever session rows still read from a damaged database.
fn salvage(conn: &Connection) -> Rows {
    let mut saved = Vec::new();
    for table in SESSION_TABLES {
        let rows = (|| -> rusqlite::Result<Vec<Vec<rusqlite::types::Value>>> {
            let mut statement = conn.prepare(&format!("SELECT * FROM {table}"))?;
            let columns = statement.column_count();
            let rows = statement.query_map([], |row| {
                (0..columns).map(|index| row.get(index)).collect()
            })?;
            rows.collect()
        })();
        if let Ok(rows) = rows {
            saved.push((table, rows));
        }
    }
    saved
}

/// Gets the damaged file out of the way: moved aside when possible (kept
/// once, for a look if it recurs), emptied in place otherwise. Windows will
/// not rename a file that anything still holds open, but it can be emptied,
/// and an empty file is a valid new database.
fn set_aside(path: &Path) {
    let name = |suffix: &str| {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        std::path::PathBuf::from(name)
    };
    let mut moved = Ok(());
    for _ in 0..10 {
        moved = std::fs::rename(path, name(".damaged"));
        if moved.is_ok() || !path.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let empty = |file: &Path| std::fs::OpenOptions::new().write(true).truncate(true).open(file).map(drop);
    let mut emptied = Ok(());
    if moved.is_err() && path.exists() {
        let _ = std::fs::copy(path, name(".damaged"));
        emptied = empty(path);
    }
    // A leftover journal would be replayed into the new database.
    for journal in [name("-wal"), name("-shm")] {
        if std::fs::remove_file(&journal).is_err() && journal.exists() {
            let _ = empty(&journal);
        }
    }
    log(path, &format!("database was damaged; moved aside: {moved:?}, emptied in place: {emptied:?}"));
}

/// Appends a line to `oxigram.log` next to the database. Rare events only.
fn log(db_path: &Path, line: &str) {
    use std::io::Write;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(db_path.with_file_name("oxigram.log"));
    if let Ok(mut file) = file {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let _ = writeln!(file, "{seconds} {line}");
    }
}

fn restore(conn: &mut Connection, saved: &Rows) {
    let Ok(tx) = conn.transaction() else { return };
    for (table, rows) in saved {
        for row in rows {
            let marks = vec!["?"; row.len()].join(", ");
            let sql = format!("INSERT OR REPLACE INTO {table} VALUES ({marks})");
            let _ = tx.execute(&sql, rusqlite::params_from_iter(row));
        }
    }
    let _ = tx.commit();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_damaged_file_is_replaced_and_still_opens() {
        let dir = std::env::temp_dir().join(format!("oxigram-damaged-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("oxigram.db");
        {
            let db = open(&path).unwrap();
            let conn = db.lock().unwrap();
            conn.execute("INSERT INTO dc_home (dc_id) VALUES (4)", []).unwrap();
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
        }
        // Scribble over everything after the first page (the schema stays).
        let mut bytes = std::fs::read(&path).unwrap();
        for byte in bytes.iter_mut().skip(4096) {
            *byte = 0xA5;
        }
        std::fs::write(&path, &bytes).unwrap();

        let db = open(&path).unwrap();
        let conn = db.lock().unwrap();
        assert!(is_healthy(&conn));
        let count: i64 = conn.query_row("SELECT count(*) FROM messages", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
        assert!(dir.join("oxigram.db.damaged").exists());
        drop(conn);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Windows refuses to rename a file that is open elsewhere; the repair
    /// must still leave a working database.
    #[test]
    fn a_damaged_file_held_open_is_emptied_in_place() {
        let dir = std::env::temp_dir().join(format!("oxigram-held-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("oxigram.db");
        {
            let db = open(&path).unwrap();
            let conn = db.lock().unwrap();
            conn.execute("INSERT INTO update_state (pts, qts, date, seq) VALUES (1, 2, 3, 4)", []).ok();
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
        }
        let mut bytes = std::fs::read(&path).unwrap();
        for byte in bytes.iter_mut().skip(8192) {
            *byte = 0xA5;
        }
        std::fs::write(&path, &bytes).unwrap();
        let holder = Connection::open(&path).unwrap();

        let db = open(&path).unwrap();
        let conn = db.lock().unwrap();
        assert!(is_healthy(&conn));
        let count: i64 = conn.query_row("SELECT count(*) FROM messages", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
        drop(conn);
        drop(db);
        drop(holder);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
