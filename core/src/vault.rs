//! The local passcode ("Код-пароль"): the key that encrypts Oxigram's data
//! on disk while a passcode is set.
//!
//! * The key is Argon2id(passcode, salt). `lock.json` keeps only the salt,
//!   the Argon2 parameters and a check block (a known text sealed with the
//!   key): no hash of the passcode, so the file alone offers nothing quicker
//!   to test guesses against than Argon2 itself.
//! * Data is sealed with ChaCha20-Poly1305 (ring): a random 12-byte nonce,
//!   then the ciphertext and its tag. A wrong key fails to open it.
//! * Files are replaced atomically (written beside, then renamed).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "lock.json";
/// What the check block holds.
const CHECK: &[u8] = b"oxigram passcode check v1";
/// Argon2id: about 0.1 s on a fast desktop and 0.3-0.5 s on a weak laptop;
/// 19 MiB while deriving (OWASP's figure), freed right after.
const MEMORY_KIB: u32 = 19 * 1024;
const PASSES: u32 = 4;
const LANES: u32 = 1;

/// The 32-byte key; wiped from memory when dropped.
pub struct Key([u8; 32]);

impl Drop for Key {
    fn drop(&mut self) {
        for byte in self.0.iter_mut() {
            // Volatile, so the wipe is not optimised away.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VaultFile {
    pub version: u32,
    /// Hex.
    pub salt: String,
    pub memory_kib: u32,
    pub passes: u32,
    pub lanes: u32,
    /// Hex: nonce, then the sealed check text.
    pub check: String,
}

#[derive(Debug, PartialEq)]
pub enum VaultError {
    /// The passcode does not open the check block.
    WrongPasscode,
    Damaged,
    Io(String),
}

impl From<io::Error> for VaultError {
    fn from(err: io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

/// A passcode is at least 4 characters (4 digits, or a password).
pub fn valid_passcode(passcode: &str) -> bool {
    passcode.chars().count() >= 4 && !passcode.chars().any(char::is_control)
}

fn derive(passcode: &str, salt: &[u8], memory_kib: u32, passes: u32, lanes: u32) -> Result<Key, VaultError> {
    let params = argon2::Params::new(memory_kib, passes, lanes, Some(32)).map_err(|_| VaultError::Damaged)?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = [0u8; 32];
    argon
        .hash_password_into(passcode.as_bytes(), salt, &mut out)
        .map_err(|_| VaultError::Damaged)?;
    Ok(Key(out))
}

fn random<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    SystemRandom::new().fill(&mut out).expect("the system's random source works");
    out
}

/// Seals `plain`: nonce, then ciphertext and tag.
pub fn seal(key: &Key, plain: &[u8]) -> Vec<u8> {
    let sealing = LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, &key.0).expect("32-byte key"));
    let nonce = random::<NONCE_LEN>();
    let mut data = plain.to_vec();
    sealing
        .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::empty(), &mut data)
        .expect("sealing does not fail");
    let mut out = Vec::with_capacity(NONCE_LEN + data.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&data);
    out
}

/// Opens what `seal` made; `None` for a wrong key or damaged data.
pub fn open(key: &Key, sealed: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < NONCE_LEN {
        return None;
    }
    let opening = LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, &key.0).ok()?);
    let nonce = Nonce::try_assume_unique_for_key(&sealed[..NONCE_LEN]).ok()?;
    let mut data = sealed[NONCE_LEN..].to_vec();
    let plain = opening.open_in_place(nonce, Aad::empty(), &mut data).ok()?;
    Some(plain.to_vec())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect()
}

/// A new vault for `passcode`: its file and key.
pub fn create(passcode: &str) -> Result<(VaultFile, Key), VaultError> {
    let salt = random::<16>();
    let key = derive(passcode, &salt, MEMORY_KIB, PASSES, LANES)?;
    let file = VaultFile {
        version: 1,
        salt: hex(&salt),
        memory_kib: MEMORY_KIB,
        passes: PASSES,
        lanes: LANES,
        check: hex(&seal(&key, CHECK)),
    };
    Ok((file, key))
}

/// The key for `passcode`, if it is the right one.
pub fn unlock(file: &VaultFile, passcode: &str) -> Result<Key, VaultError> {
    let salt = unhex(&file.salt).ok_or(VaultError::Damaged)?;
    let check = unhex(&file.check).ok_or(VaultError::Damaged)?;
    let key = derive(passcode, &salt, file.memory_kib, file.passes, file.lanes)?;
    match open(&key, &check) {
        Some(text) if text == CHECK => Ok(key),
        _ => Err(VaultError::WrongPasscode),
    }
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

pub fn load(dir: &Path) -> Option<VaultFile> {
    serde_json::from_slice(&fs::read(path(dir)).ok()?).ok()
}

pub fn save(dir: &Path, file: &VaultFile) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let raw = serde_json::to_vec_pretty(file).map_err(io::Error::other)?;
    write_atomic(&path(dir), &raw)?;
    crate::config::restrict_to_owner(&path(dir))
}

/// Writes beside the target, then renames over it: never a half-written file.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let staged = path.with_extension("tmp");
    fs::write(&staged, data)?;
    fs::rename(&staged, path)
}

/// Seals `plain` into `path`.
pub fn write_sealed(path: &Path, key: &Key, plain: &[u8]) -> io::Result<()> {
    write_atomic(path, &seal(key, plain))?;
    crate::config::restrict_to_owner(path)
}

/// Opens a sealed file; `Ok(None)` when there is no such file.
pub fn read_sealed(path: &Path, key: &Key) -> Result<Option<Vec<u8>>, VaultError> {
    match fs::read(path) {
        Ok(data) => open(key, &data).map(Some).ok_or(VaultError::Damaged),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

/// Overwrites a file with zeros before removing it (best effort: SSDs and
/// copy-on-write file systems may keep old blocks).
pub fn shred(path: &Path) {
    if let Ok(meta) = fs::metadata(path)
        && meta.is_file()
    {
        let _ = fs::write(path, vec![0u8; meta.len() as usize]);
    }
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_and_open() {
        let (_, key) = create("1234").unwrap();
        let sealed = seal(&key, b"hello");
        assert_ne!(&sealed[NONCE_LEN..], b"hello");
        assert_eq!(open(&key, &sealed).as_deref(), Some(&b"hello"[..]));
        // Two seals of the same text differ (random nonce).
        assert_ne!(seal(&key, b"hello"), sealed);
        // Tampering is caught.
        let mut bad = sealed.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert_eq!(open(&key, &bad), None);
    }

    #[test]
    fn right_and_wrong_passcode() {
        let (file, key) = create("2468").unwrap();
        assert!(!file.check.contains("oxigram"));
        let again = unlock(&file, "2468").unwrap();
        assert_eq!(again.0, key.0);
        assert_eq!(unlock(&file, "2469").err(), Some(VaultError::WrongPasscode));
        // Data sealed with the right key does not open with another.
        let sealed = seal(&key, b"secret");
        let (_, other) = create("2469").unwrap();
        assert_eq!(open(&other, &sealed), None);
    }

    #[test]
    fn file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("oxigram-vault-test-{}", std::process::id()));
        let (file, key) = create("pass word").unwrap();
        save(&dir, &file).unwrap();
        assert_eq!(load(&dir), Some(file.clone()));
        let path = dir.join("x.enc");
        write_sealed(&path, &key, b"data").unwrap();
        assert_eq!(read_sealed(&path, &key).unwrap().as_deref(), Some(&b"data"[..]));
        assert_eq!(read_sealed(&dir.join("missing"), &key).unwrap(), None);
        shred(&path);
        assert!(!path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn passcodes() {
        assert!(valid_passcode("1234"));
        assert!(valid_passcode("слово"));
        assert!(!valid_passcode("123"));
    }

    #[test]
    fn derive_takes_a_moment() {
        let started = std::time::Instant::now();
        let _ = derive("1234", &[0u8; 16], MEMORY_KIB, PASSES, LANES).unwrap();
        // Release builds take ~0.2-0.6 s; debug ones are slower.
        assert!(started.elapsed() > std::time::Duration::from_millis(20));
    }
}
