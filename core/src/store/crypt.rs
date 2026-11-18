//! Field encryption for the passcode: the SQL functions `enc(x)` and
//! `dec(x)`. The database stays an ordinary file (same page cache, no copy
//! in memory); only sensitive values are sealed, each on its own: message
//! texts, sender names, file names, entities and link previews, chat titles
//! and last-message previews, and the session's authorization keys.
//!
//! A sealed value is a BLOB: "OXE1", the type of the original value, then
//! ChaCha20-Poly1305 (`vault::seal`). Without a key `enc` keeps values as
//! they are, so the same SQL serves both modes; `dec` passes plain values
//! through, so a database half way through a migration still reads.

use std::sync::Arc;

use rusqlite::functions::FunctionFlags;
use rusqlite::types::{Value, ValueRef};
use rusqlite::Connection;

use crate::vault::{self, Key};

const MAGIC: &[u8; 4] = b"OXE1";

/// The columns sealed with a passcode: (table, column).
pub const SEALED: [(&str, &str); 8] = [
    ("dialogs", "title"),
    ("dialogs", "last_text"),
    ("messages", "sender_name"),
    ("messages", "text"),
    ("messages", "file_name"),
    ("messages", "entities"),
    ("messages", "web"),
    ("dc_option", "auth_key"),
];

fn is_sealed(value: &ValueRef<'_>) -> bool {
    matches!(value, ValueRef::Blob(b) if b.len() > 5 && &b[..4] == MAGIC)
}

fn seal_value(key: &Key, value: ValueRef<'_>) -> Value {
    let (tag, bytes): (u8, Vec<u8>) = match value {
        ValueRef::Null => return Value::Null,
        v if is_sealed(&v) => return Value::Blob(v.as_blob().map(<[u8]>::to_vec).unwrap_or_default()),
        ValueRef::Text(t) => (b'T', t.to_vec()),
        ValueRef::Blob(b) => (b'B', b.to_vec()),
        ValueRef::Integer(i) => (b'I', i.to_le_bytes().to_vec()),
        ValueRef::Real(r) => (b'R', r.to_le_bytes().to_vec()),
    };
    let mut out = Vec::with_capacity(bytes.len() + 40);
    out.extend_from_slice(MAGIC);
    out.push(tag);
    out.extend_from_slice(&vault::seal(key, &bytes));
    Value::Blob(out)
}

fn open_value(key: Option<&Key>, value: ValueRef<'_>) -> Value {
    let ValueRef::Blob(blob) = value else { return value.into() };
    if !is_sealed(&value) {
        return Value::Blob(blob.to_vec());
    }
    let tag = blob[4];
    let plain = key.and_then(|key| vault::open(key, &blob[5..]));
    match (tag, plain) {
        (b'T', Some(p)) => Value::Text(String::from_utf8_lossy(&p).into_owned()),
        (b'B', Some(p)) => Value::Blob(p),
        (b'I', Some(p)) if p.len() == 8 => Value::Integer(i64::from_le_bytes(p.try_into().unwrap_or_default())),
        (b'R', Some(p)) if p.len() == 8 => Value::Real(f64::from_le_bytes(p.try_into().unwrap_or_default())),
        // No key or the wrong one: a text column still reads (as empty).
        (b'T', _) => Value::Text(String::new()),
        _ => Value::Null,
    }
}

/// Installs `enc` and `dec` on a connection for `key` (`None`: no passcode).
pub fn use_key(conn: &Connection, key: Option<Arc<Key>>) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8;
    let for_enc = key.clone();
    conn.create_scalar_function("enc", 1, flags, move |ctx| {
        let value = ctx.get_raw(0);
        Ok(match &for_enc {
            Some(key) => seal_value(key, value),
            None => value.into(),
        })
    })?;
    conn.create_scalar_function("dec", 1, flags | FunctionFlags::SQLITE_DETERMINISTIC, move |ctx| {
        Ok(open_value(key.as_deref(), ctx.get_raw(0)))
    })?;
    // Freed pages are zeroed, so removed plain values do not linger in the file.
    conn.pragma_update(None, "secure_delete", key.is_some())?;
    Ok(())
}

/// Seals (or with `seal == false`, opens) every sealed column in place,
/// then rewrites the file so no old page keeps the other form.
pub fn convert(conn: &mut Connection, seal: bool) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    for (table, column) in SEALED {
        let function = if seal { "enc" } else { "dec" };
        tx.execute(&format!("UPDATE {table} SET {column} = {function}({column})"), [])?;
    }
    tx.commit()?;
    vacuum(conn)
}

/// Rewrites the file and empties the WAL.
pub fn vacuum(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("VACUUM")?;
    let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_roundtrip_and_pass_through() {
        let conn = Connection::open_in_memory().unwrap();
        let (_, key) = vault::create("1234").unwrap();
        use_key(&conn, Some(Arc::new(key))).unwrap();
        let text: String = conn.query_row("SELECT dec(enc('привет'))", [], |r| r.get(0)).unwrap();
        assert_eq!(text, "привет");
        let blob: Vec<u8> = conn.query_row("SELECT dec(enc(x'0102'))", [], |r| r.get(0)).unwrap();
        assert_eq!(blob, vec![1, 2]);
        let null: Option<String> = conn.query_row("SELECT dec(enc(NULL))", [], |r| r.get(0)).unwrap();
        assert_eq!(null, None);
        // Plain values pass through dec; sealing twice keeps one layer.
        let plain: String = conn.query_row("SELECT dec('plain')", [], |r| r.get(0)).unwrap();
        assert_eq!(plain, "plain");
        let twice: String = conn.query_row("SELECT dec(enc(enc('x')))", [], |r| r.get(0)).unwrap();
        assert_eq!(twice, "x");
        // The sealed form does not contain the text.
        let sealed: Vec<u8> = conn.query_row("SELECT enc('secret words')", [], |r| r.get(0)).unwrap();
        assert!(!sealed.windows(6).any(|w| w == b"secret"));
    }

    #[test]
    fn without_the_key_sealed_text_reads_empty() {
        let conn = Connection::open_in_memory().unwrap();
        let (_, key) = vault::create("1234").unwrap();
        use_key(&conn, Some(Arc::new(key))).unwrap();
        let sealed: Vec<u8> = conn.query_row("SELECT enc('secret')", [], |r| r.get(0)).unwrap();
        use_key(&conn, None).unwrap();
        let text: String = conn.query_row("SELECT dec(?1)", [sealed], |r| r.get(0)).unwrap();
        assert_eq!(text, "");
        // No key: enc is the identity.
        let same: String = conn.query_row("SELECT enc('a')", [], |r| r.get(0)).unwrap();
        assert_eq!(same, "a");
    }
}
