//! Telegram connection lifecycle and the login state machine.
//!
//! `grammers_client::Client` is a cheap, thread-safe handle; all network I/O
//! is driven by one background task (`SenderPoolRunner::run`). The mutex here
//! only orders the login steps, it is not needed to use the client.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use grammers_client::client::{DialogIter, PasswordToken};
use grammers_client::sender::{AutoSleep, ConnectionParams, SenderPool};
use grammers_client::{Client, SignInError};
use grammers_session::updates::UpdatesLike;
use serde::Serialize;
use tokio::sync::{Mutex, Semaphore, mpsc};
use tokio::task::JoinHandle;

use crate::config::{self, ApiKeys};
use crate::proxy::{self, ProxyConfig};
use crate::error::UiError;
use crate::vault;
use crate::store::{self, Db, session::SqliteSession};
use crate::updates::{self, UiSink};

const DB_FILE: &str = "oxigram.db";
const LOCK_FILE: &str = "oxigram.lock";
/// Thumbnails and avatars are small; a few at a time keeps scrolling smooth
/// without tripping flood limits.
const PARALLEL_DOWNLOADS: usize = 4;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub id: i64,
    pub name: String,
    pub username: Option<String>,
    pub phone: Option<String>,
    /// Profile photo ID, as a string: it does not fit a JavaScript number.
    pub photo_id: Option<String>,
}

/// Which screen the frontend should show.
#[derive(Clone, Serialize)]
#[serde(tag = "stage", rename_all = "camelCase")]
pub enum UiState {
    /// A passcode is set and not entered yet: nothing is decrypted, nothing
    /// connects.
    Locked,
    NeedKeys,
    NeedPhone,
    /// The code went out; how (app, SMS, call…) and whether it can be sent
    /// another way.
    NeedCode { sent: crate::login::CodeSent },
    /// Signing in by QR code: the code as path commands in a size×size box.
    NeedQr { path: String, size: i32 },
    NeedPassword { hint: Option<String> },
    Ready { me: Me },
}

enum Login {
    Idle,
    CodeSent { hash: String, sent: crate::login::CodeSent },
    /// Showing a QR code for this tg://login link.
    Qr { url: String },
    PasswordNeeded(PasswordToken),
}

struct Connection {
    client: Client,
    session: Arc<SqliteSession>,
    db: Db,
    pool_task: JoinHandle<()>,
    /// The only receiving end of the pool's update channel; handed to the
    /// update task once the account is authorized.
    updates: Option<mpsc::Receiver<UpdatesLike>>,
    update_task: Option<JoinHandle<()>>,
    me: Option<Me>,
}

struct Inner {
    keys: Option<ApiKeys>,
    connection: Option<Connection>,
    login: Login,
}

/// What the chat commands need; cheap to clone out of the lock so network
/// calls never hold it.
pub struct Handles {
    pub client: Client,
    pub session: Arc<SqliteSession>,
    pub db: Db,
}

/// Where the paged dialog list currently stands.
pub struct DialogCursor {
    pub iter: DialogIter,
    pub loaded: usize,
}

pub struct Telegram {
    config_dir: PathBuf,
    data_dir: PathBuf,
    inner: Mutex<Inner>,
    /// Separate from `inner` so paging dialogs does not block other commands.
    pub dialog_cursor: Mutex<Option<DialogCursor>>,
    /// Downloaded thumbnails and avatars.
    pub cache_dir: PathBuf,
    pub downloads: Semaphore,
    /// Where live events for the window go.
    pub ui: Arc<UiSink>,
    /// Files uploaded for an album and not sent yet, by their upload token.
    pub uploads: std::sync::Mutex<std::collections::HashMap<String, grammers_client::media::Uploaded>>,
    /// Held while the session is open, so the Tauri and the native build
    /// never use one authorization key from two processes at once.
    instance_lock: std::sync::Mutex<Option<std::fs::File>>,
    /// The proxy new connections go through.
    proxy: std::sync::Mutex<ProxyConfig>,
    /// The passcode's key while unlocked (`None` without a passcode, or locked).
    key: std::sync::Mutex<Option<Arc<vault::Key>>>,
    locked: std::sync::atomic::AtomicBool,
    /// Wrong passcodes in a row, and when the last one was entered.
    failures: std::sync::Mutex<(u32, Option<std::time::Instant>)>,
}

const KEYS_SEALED: &str = "keys.enc";
/// The 0.3.1 development format sealed the whole database; read once and
/// converted to sealed fields.
const DB_SEALED: &str = "oxigram.db.enc";

impl Telegram {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf, cache_dir: PathBuf) -> Self {
        let passcode = vault::load(&config_dir).is_some();
        // With a passcode the keys are sealed and read only after unlocking.
        let keys = if passcode { None } else { config::load_keys(&config_dir) };
        if passcode {
            // Full files (videos, originals) are kept readable for the
            // players: left by a crash, they go before anything.
            let _ = std::fs::remove_dir_all(cache_dir.join("files"));
        }
        let proxy = proxy::load(&config_dir).unwrap_or_default();
        Self {
            key: std::sync::Mutex::new(None),
            locked: std::sync::atomic::AtomicBool::new(passcode),
            failures: std::sync::Mutex::new((0, None)),
            config_dir,
            data_dir,
            inner: Mutex::new(Inner {
                keys,
                connection: None,
                login: Login::Idle,
            }),
            dialog_cursor: Mutex::new(None),
            cache_dir,
            downloads: Semaphore::new(PARALLEL_DOWNLOADS),
            ui: Arc::new(UiSink::default()),
            uploads: Default::default(),
            instance_lock: std::sync::Mutex::new(None),
            proxy: std::sync::Mutex::new(proxy),
        }
    }

    /// The saved proxy settings.
    pub fn proxy(&self) -> ProxyConfig {
        self.proxy.lock().map(|p| p.clone()).unwrap_or_default()
    }

    /// Saves the proxy and reconnects through it on the next request.
    pub async fn set_proxy(&self, config: ProxyConfig) -> Result<(), UiError> {
        config.validate()?;
        proxy::save(&self.config_dir, &config)?;
        if let Ok(mut current) = self.proxy.lock() {
            *current = config;
        }
        let mut inner = self.inner.lock().await;
        self.disconnect(&mut inner).await;
        Ok(())
    }

    pub async fn state(&self) -> Result<UiState, UiError> {
        let mut inner = self.inner.lock().await;
        self.current(&mut inner).await
    }

    /// Handles for an authorized account, or an error if nobody is signed in.
    pub async fn handles(&self) -> Result<Handles, UiError> {
        let mut inner = self.inner.lock().await;
        let connection = self.connect(&mut inner)?;
        if connection.me.is_none() {
            return Err(UiError::new("NOT_AUTHORIZED", "not signed in"));
        }
        Ok(Handles {
            client: connection.client.clone(),
            session: Arc::clone(&connection.session),
            db: Arc::clone(&connection.db),
        })
    }

    pub async fn save_keys(&self, api_id: i32, api_hash: &str) -> Result<UiState, UiError> {
        let keys = ApiKeys::parse(api_id, api_hash)
            .ok_or_else(|| UiError::new("KEYS_INVALID", "malformed api_id or api_hash"))?;

        let mut inner = self.inner.lock().await;
        if inner.keys.as_ref() == Some(&keys) {
            return self.current(&mut inner).await;
        }

        // Authorization keys are bound to the api_id they were created with.
        self.forget_account(&mut inner).await?;
        // Telegram only validates the keys when a login code is requested
        // (API_ID_INVALID), so nothing more can be checked at this point.
        self.store_keys(&keys)?;
        inner.keys = Some(keys);
        self.current(&mut inner).await
    }

    pub async fn clear_keys(&self) -> Result<UiState, UiError> {
        let mut inner = self.inner.lock().await;
        self.forget_account(&mut inner).await?;
        config::clear_keys(&self.config_dir)?;
        vault::shred(&self.config_dir.join(KEYS_SEALED));
        inner.keys = None;
        Ok(UiState::NeedKeys)
    }

    pub async fn request_code(&self, phone: &str) -> Result<UiState, UiError> {
        let phone: String = phone
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '+')
            .collect();
        if phone.trim_start_matches('+').len() < 5 {
            return Err(UiError::new("PHONE_NUMBER_INVALID", "phone number too short"));
        }

        let mut inner = self.inner.lock().await;
        let (api_id, api_hash) = match &inner.keys {
            Some(keys) => (keys.api_id, keys.api_hash.clone()),
            None => return Ok(UiState::NeedKeys),
        };
        let connection = self.connect(&mut inner)?;
        let (client, session) = (connection.client.clone(), Arc::clone(&connection.session));
        let (hash, sent) = crate::login::send_code(&client, &session, &phone, api_id, &api_hash).await?;
        inner.login = Login::CodeSent { hash, sent: sent.clone() };
        Ok(UiState::NeedCode { sent })
    }

    /// "Send the code another way" (auth.resendCode).
    pub async fn resend_code(&self) -> Result<UiState, UiError> {
        let mut inner = self.inner.lock().await;
        let Login::CodeSent { hash, sent } = &inner.login else {
            return Err(wrong_stage());
        };
        let (hash, phone) = (hash.clone(), sent.phone.clone());
        let client = self.connect(&mut inner)?.client.clone();
        let (hash, sent) = crate::login::resend_code(&client, &phone, &hash).await?;
        inner.login = Login::CodeSent { hash, sent: sent.clone() };
        Ok(UiState::NeedCode { sent })
    }

    /// Signing in by QR code: asked again every few seconds while the code
    /// is on screen; the code (new when the token changed), or the next step
    /// once it was scanned.
    pub async fn qr_login(&self) -> Result<UiState, UiError> {
        let mut inner = self.inner.lock().await;
        let (api_id, api_hash) = match &inner.keys {
            Some(keys) => (keys.api_id, keys.api_hash.clone()),
            None => return Ok(UiState::NeedKeys),
        };
        if matches!(inner.login, Login::PasswordNeeded(_)) {
            return self.current(&mut inner).await;
        }
        let connection = self.connect(&mut inner)?;
        let (client, session) = (connection.client.clone(), Arc::clone(&connection.session));
        match crate::login::qr(&client, &session, api_id, &api_hash).await? {
            crate::login::Qr::Show { url, .. } => {
                inner.login = Login::Qr { url };
                self.current(&mut inner).await
            }
            crate::login::Qr::Done(crate::login::Signed::In) => {
                inner.login = Login::Idle;
                self.current(&mut inner).await
            }
            crate::login::Qr::Done(crate::login::Signed::Password(token)) => {
                let hint = token.hint().map(str::to_owned);
                inner.login = Login::PasswordNeeded(token);
                Ok(UiState::NeedPassword { hint })
            }
        }
    }

    pub async fn submit_code(&self, code: &str) -> Result<UiState, UiError> {
        let mut inner = self.inner.lock().await;
        let Login::CodeSent { hash, sent } = std::mem::replace(&mut inner.login, Login::Idle) else {
            return Err(wrong_stage());
        };
        let connection = self.connect(&mut inner)?;
        let (client, session) = (connection.client.clone(), Arc::clone(&connection.session));

        match crate::login::sign_in(&client, &session, &sent.phone, &hash, code.trim()).await {
            Ok(crate::login::Signed::In) => self.current(&mut inner).await,
            Ok(crate::login::Signed::Password(password_token)) => {
                let hint = password_token.hint().map(str::to_owned);
                inner.login = Login::PasswordNeeded(password_token);
                Ok(UiState::NeedPassword { hint })
            }
            Err(err) => {
                // An expired code cannot be retried; anything else can.
                if err.code != "PHONE_CODE_EXPIRED" {
                    inner.login = Login::CodeSent { hash, sent };
                }
                Err(err)
            }
        }
    }

    pub async fn submit_password(&self, password: &str) -> Result<UiState, UiError> {
        let mut inner = self.inner.lock().await;
        let Login::PasswordNeeded(token) = std::mem::replace(&mut inner.login, Login::Idle) else {
            return Err(wrong_stage());
        };
        let client = self.connect(&mut inner)?.client.clone();

        match client.check_password(token, password).await {
            Ok(_) => self.current(&mut inner).await,
            Err(SignInError::InvalidPassword(token)) => {
                inner.login = Login::PasswordNeeded(token);
                Err(UiError::new("PASSWORD_INVALID", "invalid password"))
            }
            // The token is consumed by the attempt, so the login starts over.
            Err(err) => Err(err.into()),
        }
    }

    pub async fn back_to_phone(&self) -> Result<UiState, UiError> {
        let mut inner = self.inner.lock().await;
        inner.login = Login::Idle;
        self.current(&mut inner).await
    }

    pub async fn sign_out(&self) -> Result<UiState, UiError> {
        let mut inner = self.inner.lock().await;
        if let Some(connection) = &inner.connection {
            // Only forget the session locally once Telegram has revoked it,
            // otherwise it would stay active with no way to reach it.
            connection.client.sign_out().await?;
        }
        self.forget_account(&mut inner).await?;
        self.current(&mut inner).await
    }

    /// Resolves the screen to show, connecting and asking Telegram whether
    /// the stored session is still authorized when needed.
    async fn current(&self, inner: &mut Inner) -> Result<UiState, UiError> {
        if self.is_locked() {
            return Ok(UiState::Locked);
        }
        if inner.keys.is_none() {
            return Ok(UiState::NeedKeys);
        }
        match &inner.login {
            Login::CodeSent { sent, .. } => {
                return Ok(UiState::NeedCode { sent: sent.clone() });
            }
            Login::Qr { url } => {
                let (path, size) = crate::login::qr_path(url).unwrap_or_default();
                return Ok(UiState::NeedQr { path, size });
            }
            Login::PasswordNeeded(token) => {
                return Ok(UiState::NeedPassword {
                    hint: token.hint().map(str::to_owned),
                });
            }
            Login::Idle => {}
        }

        let connection = self.connect(inner)?;
        if let Some(me) = &connection.me {
            return Ok(UiState::Ready { me: me.clone() });
        }
        if !connection.client.is_authorized().await? {
            return Ok(UiState::NeedPhone);
        }
        let user = connection.client.get_me().await?;
        let me = Me {
            id: user.id().bare_id().unwrap_or_default(),
            name: user.full_name(),
            username: user.username().map(str::to_owned),
            phone: user.phone().map(str::to_owned),
            photo_id: user.photo().map(|photo| photo.photo_id.to_string()),
        };
        connection.me = Some(me.clone());
        if let Some(receiver) = connection.updates.take() {
            connection.update_task = Some(tokio::spawn(updates::run(
                connection.client.clone(),
                receiver,
                Arc::clone(&connection.db),
                Arc::clone(&self.ui),
            )));
        }
        Ok(UiState::Ready { me })
    }

    /// Returns the live connection, creating it on first use. Sockets are
    /// opened lazily by the pool, so this does not touch the network.
    fn connect<'a>(&self, inner: &'a mut Inner) -> Result<&'a mut Connection, UiError> {
        // Locked: nothing is opened (and no readable database is created).
        if self.is_locked() {
            return Err(UiError::new("LOCKED", "enter the passcode first"));
        }
        let keys = inner
            .keys
            .as_ref()
            .ok_or_else(|| UiError::new("KEYS_MISSING", "API keys are not set"))?;

        if inner.connection.is_none() {
            let (db, session) = self.open_session()?;
            let session = Arc::new(session);
            let params = ConnectionParams {
                app_version: env!("CARGO_PKG_VERSION").to_owned(),
                // The default policy sleeps through flood waits of up to a
                // minute, which would look like a frozen button. Wait only for
                // short ones and report the rest to the user.
                retry_policy: Box::new(AutoSleep {
                    tries: NonZeroU32::new(2).unwrap_or(NonZeroU32::MIN),
                    threshold: Duration::from_secs(3),
                    io_errors_as_flood_of: Some(Duration::from_secs(1)),
                }),
                proxy_url: self.proxy.lock().ok().and_then(|p| p.connection_url()),
                // Telegram's WebSocket endpoints take only the obfuscated
                // transport, as Telegram Web sends it.
                transport: if self.proxy.lock().ok().is_some_and(|p| p.active() && p.is_websocket()) {
                    Box::new(|| {
                        Box::new(grammers_mtproto::transport::Obfuscated::new(
                            grammers_mtproto::transport::Intermediate::new(),
                        ))
                    })
                } else {
                    Box::new(|| Box::new(grammers_mtproto::transport::Full::new()))
                },
                ..Default::default()
            };
            let SenderPool {
                runner,
                handle,
                updates,
            } = SenderPool::with_configuration(Arc::clone(&session), keys.api_id, params);

            inner.connection = Some(Connection {
                client: Client::new(handle),
                session,
                db,
                pool_task: tokio::spawn(runner.run()),
                updates: Some(updates),
                update_task: None,
                me: None,
            });
        }
        inner
            .connection
            .as_mut()
            .ok_or_else(|| UiError::internal("connection unavailable"))
    }

    fn open_session(&self) -> Result<(Db, SqliteSession), UiError> {
        std::fs::create_dir_all(&self.data_dir)?;
        self.lock_instance()?;
        let db = store::open(self.data_dir.join(DB_FILE))?;
        if let Some(key) = self.key() {
            // A passcode: sensitive fields are sealed one by one.
            let conn = db.lock().map_err(|_| UiError::internal("database lock"))?;
            store::crypt::use_key(&conn, Some(key))?;
        }
        let session = SqliteSession::load(Arc::clone(&db)).map_err(UiError::internal)?;
        Ok((db, session))
    }

    /// Takes the per-account lock file, once per process.
    fn lock_instance(&self) -> Result<(), UiError> {
        let mut slot = self
            .instance_lock
            .lock()
            .map_err(|_| UiError::internal("instance lock is poisoned"))?;
        if slot.is_some() {
            return Ok(());
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.data_dir.join(LOCK_FILE))?;
        match file.try_lock() {
            Ok(()) => {
                *slot = Some(file);
                Ok(())
            }
            Err(std::fs::TryLockError::WouldBlock) => Err(UiError::new(
                "ALREADY_RUNNING",
                "Oxigram is already running with this account",
            )),
            Err(std::fs::TryLockError::Error(err)) => Err(err.into()),
        }
    }

    /// Stops the network task and returns the session it was using.
    async fn disconnect(&self, inner: &mut Inner) -> Option<Arc<SqliteSession>> {
        inner.login = Login::Idle;
        *self.dialog_cursor.lock().await = None;
        let connection = inner.connection.take()?;
        if let Some(task) = connection.update_task {
            task.abort();
            let _ = task.await;
        }
        connection.client.disconnect();
        connection.pool_task.abort();
        // Wait until the runner has really stopped using the session.
        let _ = connection.pool_task.await;
        Some(connection.session)
    }

    /// Drops the connection and erases everything tied to the account.
    async fn forget_account(&self, inner: &mut Inner) -> Result<(), UiError> {
        let session = match self.disconnect(inner).await {
            Some(session) => session,
            None => Arc::new(self.open_session()?.1),
        };
        session.wipe().map_err(UiError::internal)?;
        // Cached pictures belong to the account too.
        match std::fs::remove_dir_all(&self.cache_dir) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err.into()),
            _ => Ok(()),
        }
    }
}

// ---- The local passcode ------------------------------------------------------

fn vault_error(err: vault::VaultError) -> UiError {
    match err {
        vault::VaultError::WrongPasscode => UiError::new("PASSCODE_WRONG", "wrong passcode"),
        vault::VaultError::Damaged => UiError::new("PASSCODE_DAMAGED", "the encrypted data cannot be read"),
        vault::VaultError::Io(message) => UiError::new("STORAGE", message),
    }
}

/// Seconds to wait after `failures` wrong passcodes in a row: none for the
/// first three, then 1, 2, 4… up to 60.
pub fn passcode_delay(failures: u32) -> u64 {
    if failures < 3 { 0 } else { (1u64 << (failures - 3).min(6)).min(60) }
}

impl Telegram {
    pub fn passcode_on(&self) -> bool {
        vault::path(&self.config_dir).is_file()
    }

    pub fn is_locked(&self) -> bool {
        self.locked.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn key(&self) -> Option<Arc<vault::Key>> {
        self.key.lock().ok().and_then(|k| k.clone())
    }

    fn set_key(&self, key: Option<vault::Key>) {
        let key = key.map(Arc::new);
        vault::set_media_key(key.clone());
        if let Ok(mut slot) = self.key.lock() {
            *slot = key;
        }
    }

    /// The API keys: sealed with a passcode, plain without.
    fn store_keys(&self, keys: &ApiKeys) -> Result<(), UiError> {
        match self.key() {
            Some(key) => {
                std::fs::create_dir_all(&self.config_dir)?;
                let raw = serde_json::to_vec(keys).map_err(UiError::internal)?;
                vault::write_sealed(&self.config_dir.join(KEYS_SEALED), &key, &raw)?;
            }
            None => config::save_keys(&self.config_dir, keys)?,
        }
        Ok(())
    }

    fn sealed_keys(&self, key: &vault::Key) -> Result<Option<ApiKeys>, UiError> {
        let raw = vault::read_sealed(&self.config_dir.join(KEYS_SEALED), key).map_err(vault_error)?;
        Ok(raw.and_then(|raw| serde_json::from_slice(&raw).ok()))
    }

    /// Opens the database file with `key` installed for enc() / dec().
    fn open_with(&self, key: Option<Arc<vault::Key>>) -> Result<Db, UiError> {
        std::fs::create_dir_all(&self.data_dir)?;
        let db = store::open(self.data_dir.join(DB_FILE))?;
        {
            let conn = db.lock().map_err(|_| UiError::internal("database lock"))?;
            store::crypt::use_key(&conn, key)?;
        }
        Ok(db)
    }

    /// Seals (`true`) or opens (`false`) the sensitive fields of the file.
    fn convert_db(&self, key: Arc<vault::Key>, seal: bool) -> Result<(), UiError> {
        let db = self.open_with(Some(key))?;
        let mut conn = db.lock().map_err(|_| UiError::internal("database lock"))?;
        store::crypt::convert(&mut conn, seal)?;
        Ok(())
    }

    /// Enters the passcode. Wrong ones in a row make the next try wait.
    /// Then `state` connects.
    pub async fn unlock(&self, passcode: &str) -> Result<(), UiError> {
        let Some(file) = vault::load(&self.config_dir) else {
            self.locked.store(false, std::sync::atomic::Ordering::SeqCst);
            return Ok(());
        };
        if let Ok(failures) = self.failures.lock()
            && let (count, Some(at)) = *failures
        {
            let wait = passcode_delay(count);
            let passed = at.elapsed().as_secs();
            if passed < wait {
                let mut err = UiError::new("PASSCODE_WAIT", "too many wrong passcodes");
                err.wait_seconds = Some((wait - passed) as u32);
                return Err(err);
            }
        }
        let passcode = passcode.to_owned();
        let result = tokio::task::spawn_blocking(move || vault::unlock(&file, &passcode))
            .await
            .map_err(UiError::internal)?;
        let key = match result {
            Ok(key) => key,
            Err(err) => {
                if err == vault::VaultError::WrongPasscode
                    && let Ok(mut failures) = self.failures.lock()
                {
                    *failures = (failures.0 + 1, Some(std::time::Instant::now()));
                }
                return Err(vault_error(err));
            }
        };
        if let Ok(mut failures) = self.failures.lock() {
            *failures = (0, None);
        }
        let keys = self.sealed_keys(&key)?;
        let key = Arc::new(key);
        // A database sealed whole (0.3.1 development builds): back to a file
        // with sealed fields.
        let whole = self.data_dir.join(DB_SEALED);
        if let Some(image) = vault::read_sealed(&whole, &key).map_err(vault_error)? {
            store::write_file(image, &self.data_dir.join(DB_FILE))?;
            vault::shred(&whole);
            self.convert_db(Arc::clone(&key), true)?;
        }
        vault::set_media_key(Some(Arc::clone(&key)));
        if let Ok(mut slot) = self.key.lock() {
            *slot = Some(key);
        }
        self.locked.store(false, std::sync::atomic::Ordering::SeqCst);
        let mut inner = self.inner.lock().await;
        inner.keys = keys;
        Ok(())
    }

    /// Locks: seals the database, drops the connection, the keys and the
    /// key, and removes downloaded media.
    pub async fn lock(&self) -> Result<UiState, UiError> {
        if !self.passcode_on() {
            return self.state().await;
        }
        let mut inner = self.inner.lock().await;
        self.disconnect(&mut inner).await;
        inner.keys = None;
        self.set_key(None);
        self.locked.store(true, std::sync::atomic::Ordering::SeqCst);
        // Pictures are sealed and stay; full files are readable and go.
        let _ = std::fs::remove_dir_all(self.cache_dir.join("files"));
        Ok(UiState::Locked)
    }

    /// Turns the passcode on: seals the database and the API keys, then
    /// removes their readable files and the downloaded media.
    pub async fn enable_passcode(&self, passcode: &str) -> Result<(), UiError> {
        if self.passcode_on() {
            return Err(UiError::new("PASSCODE_SET", "a passcode is already set"));
        }
        if !vault::valid_passcode(passcode) {
            return Err(UiError::new("PASSCODE_SHORT", "at least 4 characters"));
        }
        let passcode = passcode.to_owned();
        let (file, key) = tokio::task::spawn_blocking(move || vault::create(&passcode))
            .await
            .map_err(UiError::internal)?
            .map_err(vault_error)?;
        let mut inner = self.inner.lock().await;
        self.disconnect(&mut inner).await;
        let key = Arc::new(key);
        if let Some(keys) = &inner.keys {
            let raw = serde_json::to_vec(keys).map_err(UiError::internal)?;
            vault::write_sealed(&self.config_dir.join(KEYS_SEALED), &key, &raw)?;
        }
        // Sealing the fields, then the vault: a crash between leaves fields
        // sealed with a key that a retry would not have; so the vault first,
        // and dec() still reads any field left plain.
        vault::save(&self.config_dir, &file)?;
        if self.data_dir.join(DB_FILE).exists() {
            self.convert_db(Arc::clone(&key), true)?;
        }
        vault::shred(&self.config_dir.join("config.json"));
        self.clear_cache();
        vault::set_media_key(Some(Arc::clone(&key)));
        if let Ok(mut slot) = self.key.lock() {
            *slot = Some(key);
        }
        Ok(())
    }

    /// Checks a passcode without changing anything.
    async fn verify(&self, passcode: &str) -> Result<vault::Key, UiError> {
        let file = vault::load(&self.config_dir).ok_or_else(|| UiError::new("PASSCODE_NONE", "no passcode is set"))?;
        let passcode = passcode.to_owned();
        tokio::task::spawn_blocking(move || vault::unlock(&file, &passcode))
            .await
            .map_err(UiError::internal)?
            .map_err(vault_error)
    }

    /// Turns the passcode off: the database and the keys become plain files again.
    pub async fn disable_passcode(&self, passcode: &str) -> Result<(), UiError> {
        let key = self.verify(passcode).await?;
        let mut inner = self.inner.lock().await;
        self.disconnect(&mut inner).await;
        let key = Arc::new(key);
        if let Some(image) = vault::read_sealed(&self.data_dir.join(DB_SEALED), &key).map_err(vault_error)? {
            store::write_file(image, &self.data_dir.join(DB_FILE))?;
        } else if self.data_dir.join(DB_FILE).exists() {
            self.convert_db(Arc::clone(&key), false)?;
        }
        if let Some(keys) = self.sealed_keys(&key)? {
            config::save_keys(&self.config_dir, &keys)?;
        }
        vault::shred(&vault::path(&self.config_dir));
        vault::shred(&self.data_dir.join(DB_SEALED));
        vault::shred(&self.config_dir.join(KEYS_SEALED));
        self.set_key(None);
        // Sealed pictures would no longer open.
        self.clear_cache();
        Ok(())
    }

    /// A new passcode: everything is sealed again with the new key.
    pub async fn change_passcode(&self, old: &str, new: &str) -> Result<(), UiError> {
        if !vault::valid_passcode(new) {
            return Err(UiError::new("PASSCODE_SHORT", "at least 4 characters"));
        }
        let old_key = self.verify(old).await?;
        let new_passcode = new.to_owned();
        let (file, key) = tokio::task::spawn_blocking(move || vault::create(&new_passcode))
            .await
            .map_err(UiError::internal)?
            .map_err(vault_error)?;
        let mut inner = self.inner.lock().await;
        self.disconnect(&mut inner).await;
        let keys_path = self.config_dir.join(KEYS_SEALED);
        let keys = vault::read_sealed(&keys_path, &old_key).map_err(vault_error)?;
        let (old_key, key) = (Arc::new(old_key), Arc::new(key));
        if self.data_dir.join(DB_FILE).exists() {
            // Opened with the old key, sealed with the new one, in one go.
            let db = self.open_with(Some(old_key))?;
            let mut conn = db.lock().map_err(|_| UiError::internal("database lock"))?;
            let tx = conn.transaction()?;
            for (table, column) in store::crypt::SEALED {
                tx.execute(&format!("UPDATE {table} SET {column} = dec({column})"), [])?;
            }
            store::crypt::use_key(&tx, Some(Arc::clone(&key)))?;
            for (table, column) in store::crypt::SEALED {
                tx.execute(&format!("UPDATE {table} SET {column} = enc({column})"), [])?;
            }
            tx.commit()?;
            store::crypt::vacuum(&conn)?;
        }
        if let Some(keys) = keys {
            vault::write_sealed(&keys_path, &key, &keys)?;
        }
        vault::save(&self.config_dir, &file)?;
        // Pictures sealed with the old key would no longer open.
        self.clear_cache();
        vault::set_media_key(Some(Arc::clone(&key)));
        if let Ok(mut slot) = self.key.lock() {
            *slot = Some(key);
        }
        Ok(())
    }

    /// "Забыли код?": the encrypted data cannot be opened, so it is deleted
    /// (the account is signed out locally; its session stays listed in
    /// Telegram's active sessions until ended from another device).
    pub async fn forget_passcode(&self) -> Result<UiState, UiError> {
        let mut inner = self.inner.lock().await;
        self.set_key(None);
        vault::set_media_key(None);
        self.disconnect(&mut inner).await;
        inner.keys = None;
        for path in [
            vault::path(&self.config_dir),
            self.config_dir.join(KEYS_SEALED),
            self.data_dir.join(DB_SEALED),
            self.data_dir.join(DB_FILE),
            self.data_dir.join("oxigram.db-wal"),
            self.data_dir.join("oxigram.db-shm"),
            self.config_dir.join("config.json"),
        ] {
            vault::shred(&path);
        }
        self.clear_cache();
        if let Ok(mut failures) = self.failures.lock() {
            *failures = (0, None);
        }
        self.locked.store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(UiState::NeedKeys)
    }
}

impl Telegram {
    /// Bytes of downloaded pictures, avatars and media kept on disk.
    pub fn cache_size(&self) -> u64 {
        fn walk(dir: &std::path::Path) -> u64 {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return 0;
            };
            entries
                .filter_map(Result::ok)
                .map(|entry| match entry.metadata() {
                    Ok(meta) if meta.is_dir() => walk(&entry.path()),
                    Ok(meta) => meta.len(),
                    Err(_) => 0,
                })
                .sum()
        }
        walk(&self.cache_dir)
    }

    /// Deletes downloaded media; it is fetched again when needed. Files in
    /// use (a video playing right now) are skipped.
    pub fn clear_cache(&self) {
        fn walk(dir: &std::path::Path) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path);
                    let _ = std::fs::remove_dir(&path);
                } else {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        walk(&self.cache_dir);
    }
}

fn wrong_stage() -> UiError {
    UiError::new("WRONG_STAGE", "this login step is no longer pending")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialog(title: &str) -> store::chats::DialogRow {
        store::chats::DialogRow {
            id: 42,
            kind: "user".into(),
            title: title.into(),
            last_text: "last words".into(),
            last_date: 0,
            last_outgoing: false,
            last_id: 0,
            unread: 0,
            pinned: false,
            archived: false,
            photo_id: None,
            read_outbox: 0,
            unread_reactions: 0,
            muted: false,
            contact: false,
            bot: false,
            presence: None,
            can_post: true,
        }
    }

    fn file_has(dir: &std::path::Path, needle: &[u8]) -> bool {
        ["oxigram.db", "oxigram.db-wal"].iter().any(|name| {
            std::fs::read(dir.join(name)).is_ok_and(|bytes| bytes.windows(needle.len()).any(|w| w == needle))
        })
    }

    /// The passcode seals and opens fields in the database file in place.
    #[tokio::test(flavor = "multi_thread")]
    async fn passcode_seals_and_restores_the_data() {
        let _turn = vault::MEDIA_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("oxigram-pin-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (config_dir, data_dir, cache_dir) = (dir.join("config"), dir.join("data"), dir.join("cache"));
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(cache_dir.join("avatars")).unwrap();
        std::fs::write(cache_dir.join("avatars").join("a.jpg"), b"picture").unwrap();
        let keys = ApiKeys::parse(1, "0123456789abcdef0123456789abcdef").unwrap();
        config::save_keys(&config_dir, &keys).unwrap();
        {
            let db = store::open(data_dir.join(DB_FILE)).unwrap();
            let mut conn = db.lock().unwrap();
            store::chats::save_dialogs(&mut conn, 0, &[dialog("very-secret-title")]).unwrap();
        }
        assert!(file_has(&data_dir, b"very-secret-title"));

        // On: the fields are sealed in the file, media is removed.
        let telegram = Telegram::new(config_dir.clone(), data_dir.clone(), cache_dir.clone());
        telegram.enable_passcode("1234").await.unwrap();
        assert!(data_dir.join(DB_FILE).exists());
        assert!(!file_has(&data_dir, b"very-secret-title"));
        assert!(!config_dir.join("config.json").exists());
        assert!(!cache_dir.join("avatars").join("a.jpg").exists());
        let sealed_keys = std::fs::read(config_dir.join(KEYS_SEALED)).unwrap();
        assert!(!sealed_keys.windows(32).any(|w| w == b"0123456789abcdef0123456789abcdef"));
        drop(telegram);

        // A new start is locked; a wrong passcode is refused, the right one opens.
        let telegram = Telegram::new(config_dir.clone(), data_dir.clone(), cache_dir.clone());
        assert!(telegram.is_locked());
        assert!(matches!(telegram.state().await, Ok(UiState::Locked)));
        assert_eq!(telegram.unlock("0000").await.err().map(|e| e.code).as_deref(), Some("PASSCODE_WRONG"));
        telegram.unlock("1234").await.unwrap();
        assert!(!telegram.is_locked());
        assert!(telegram.inner.lock().await.keys.as_ref() == Some(&keys));
        {
            let (db, _) = telegram.open_session().unwrap();
            let rows = store::chats::load_dialogs(&db.lock().unwrap()).unwrap();
            assert_eq!(rows[0].title, "very-secret-title");
            assert_eq!(rows[0].last_text, "last words");
        }

        // A new passcode; the old one no longer opens, the data still reads.
        telegram.change_passcode("1234", "abcd").await.unwrap();
        assert!(telegram.verify("1234").await.is_err());
        assert!(!file_has(&data_dir, b"very-secret-title"));
        {
            let (db, _) = telegram.open_session().unwrap();
            let rows = store::chats::load_dialogs(&db.lock().unwrap()).unwrap();
            assert_eq!(rows[0].title, "very-secret-title");
        }

        // Off: the fields are plain again.
        telegram.disable_passcode("abcd").await.unwrap();
        assert!(!vault::path(&config_dir).exists());
        assert!(config::load_keys(&config_dir) == Some(keys));
        let db = store::open(data_dir.join(DB_FILE)).unwrap();
        let rows = store::chats::load_dialogs(&db.lock().unwrap()).unwrap();
        assert_eq!(rows[0].title, "very-secret-title");
        drop(db);
        drop(telegram);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrong_passcodes_wait_longer() {
        assert_eq!(passcode_delay(2), 0);
        assert_eq!(passcode_delay(3), 1);
        assert_eq!(passcode_delay(5), 4);
        assert_eq!(passcode_delay(20), 60);
    }

    /// Talks to Telegram's production servers, so it only runs on request:
    /// `cargo test -- --ignored`.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs network access to Telegram"]
    async fn made_up_keys_are_refused_when_requesting_a_code() {
        let dir = std::env::temp_dir().join(format!("oxigram-tg-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let telegram = Telegram::new(dir.join("config"), dir.join("data"), dir.join("cache"));

        assert!(matches!(telegram.state().await, Ok(UiState::NeedKeys)));

        let malformed = telegram.save_keys(1, "not-a-hash").await;
        assert_eq!(
            malformed.err().map(|err| err.code).as_deref(),
            Some("KEYS_INVALID")
        );

        // Well-formed keys are accepted locally; a fresh session is not authorized.
        let saved = telegram
            .save_keys(1, "0123456789abcdef0123456789abcdef")
            .await;
        assert!(matches!(saved, Ok(UiState::NeedPhone)), "save failed");

        // +999 is not an assigned country code, so no real phone is involved.
        let refused = telegram.request_code("+99966 2 0000").await;
        assert_eq!(
            refused.err().map(|err| err.code).as_deref(),
            Some("API_ID_INVALID")
        );

        // Changing keys erases the session and the saved keys.
        assert!(matches!(telegram.clear_keys().await, Ok(UiState::NeedKeys)));
        assert!(config::load_keys(&dir.join("config")).is_none());

        drop(telegram);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
