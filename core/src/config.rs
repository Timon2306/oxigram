//! User-supplied Telegram API keys, kept in a local config file.
//!
//! Oxigram never ships with an `api_id`: every user obtains their own at
//! <https://my.telegram.org> and types it into the first-run screen.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "config.json";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKeys {
    pub api_id: i32,
    pub api_hash: String,
}

impl ApiKeys {
    /// Checks the shape of the keys; whether Telegram accepts them is only
    /// known once the first login request is made.
    pub fn parse(api_id: i32, api_hash: &str) -> Option<Self> {
        let api_hash = api_hash.trim().to_ascii_lowercase();
        let hash_ok = api_hash.len() == 32 && api_hash.bytes().all(|b| b.is_ascii_hexdigit());
        (api_id > 0 && hash_ok).then_some(Self { api_id, api_hash })
    }
}

#[derive(Default, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(flatten)]
    keys: Option<ApiKeys>,
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// Returns `None` when no keys were saved yet or the file is unreadable,
/// in which case the user is simply asked for the keys again.
pub fn load_keys(dir: &Path) -> Option<ApiKeys> {
    let raw = fs::read(path(dir)).ok()?;
    let file: ConfigFile = serde_json::from_slice(&raw).ok()?;
    file.keys
        .and_then(|keys| ApiKeys::parse(keys.api_id, &keys.api_hash))
}

pub fn save_keys(dir: &Path, keys: &ApiKeys) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let file = ConfigFile {
        keys: Some(keys.clone()),
    };
    let raw = serde_json::to_vec_pretty(&file).map_err(io::Error::other)?;
    let path = path(dir);
    fs::write(&path, raw)?;
    restrict_to_owner(&path)
}

pub fn clear_keys(dir: &Path) -> io::Result<()> {
    match fs::remove_file(path(dir)) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

#[cfg(unix)]
pub(crate) fn restrict_to_owner(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

// On Windows the per-user app data directory is already private to the account.
#[cfg(not(unix))]
pub(crate) fn restrict_to_owner(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_validates_shape() {
        let hash = "0123456789abcdef0123456789ABCDEF";
        let keys = ApiKeys::parse(12345, &format!("  {hash} ")).expect("valid keys");
        assert_eq!(keys.api_hash, hash.to_ascii_lowercase());

        assert!(ApiKeys::parse(0, hash).is_none());
        assert!(ApiKeys::parse(-1, hash).is_none());
        assert!(ApiKeys::parse(1, "abc").is_none());
        assert!(ApiKeys::parse(1, "z123456789abcdef0123456789abcdef").is_none());
    }

    #[test]
    fn save_load_clear_roundtrip() {
        let dir = std::env::temp_dir().join(format!("oxigram-config-test-{}", std::process::id()));
        let keys = ApiKeys::parse(42, "0123456789abcdef0123456789abcdef").unwrap();

        assert!(load_keys(&dir).is_none());
        save_keys(&dir, &keys).unwrap();
        assert!(load_keys(&dir) == Some(keys));
        clear_keys(&dir).unwrap();
        assert!(load_keys(&dir).is_none());
        clear_keys(&dir).unwrap();

        let _ = fs::remove_dir_all(&dir);
    }
}
