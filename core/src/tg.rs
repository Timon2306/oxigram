//! Telegram connection lifecycle and the login state machine.
//!
//! `grammers_client::Client` is a cheap, thread-safe handle; all network I/O
//! is driven by one background task (`SenderPoolRunner::run`). The mutex here
//! only orders the login steps, it is not needed to use the client.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use grammers_client::client::{DialogIter, LoginToken, PasswordToken};
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
    NeedCode { phone: String },
    NeedPassword { hint: Option<String> },
    Ready { me: Me },
}

enum Login {
    Idle,
    CodeSent { token: LoginToken, phone: String },
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
    /// With a passcode: seals the in-memory database to disk now and then.
    flusher: Option<JoinHandle<()>>,
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
const DB_SEALED: &str = "oxigram.db.enc";
/// How often the in-memory database is sealed to disk while it changes.
const FLUSH_EVERY: Duration = Duration::from_secs(30);

/// Seals the in-memory database into `path` (skipped when nothing changed
/// since `last`).
fn seal_db(db: &Db, key: &vault::Key, path: &std::path::Path, last: Option<&std::sync::atomic::AtomicU64>) {
    let Ok(mut conn) = db.lock() else { return };
    let changes = conn.total_changes();
    if let Some(last) = last
        && last.swap(changes, std::sync::atomic::Ordering::Relaxed) == changes
    {
        return;
    }
    if let Ok(image) = store::image(&mut conn) {
        drop(conn);
        let _ = vault::write_sealed(path, key, &image);
    }
}

impl Telegram {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf, cache_dir: PathBuf) -> Self {
        let passcode = vault::load(&config_dir).is_some();
        // With a passcode the keys are sealed and read only after unlocking.
        let keys = if passcode { None } else { config::load_keys(&config_dir) };
        if passcode {
            // Media left by a crash would be readable: it goes before anything.
            let _ = std::fs::remove_dir_all(&cache_dir);
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
        let api_hash = match &inner.keys {
            Some(keys) => keys.api_hash.clone(),
            None => return Ok(UiState::NeedKeys),
        };
        let client = self.connect(&mut inner)?.client.clone();
        let token = client.request_login_code(&phone, &api_hash).await?;
        inner.login = Login::CodeSent {
            token,
            phone: phone.clone(),
        };
        Ok(UiState::NeedCode { phone })
    }

    pub async fn submit_code(&self, code: &str) -> Result<UiState, UiError> {
        let mut inner = self.inner.lock().await;
        let Login::CodeSent { token, phone } = std::mem::replace(&mut inner.login, Login::Idle)
        else {
            return Err(wrong_stage());
        };
        let client = self.connect(&mut inner)?.client.clone();

        match client.sign_in(&token, code.trim()).await {
            Ok(_) => self.current(&mut inner).await,
            Err(SignInError::PasswordRequired(password_token)) => {
                let hint = password_token.hint().map(str::to_owned);
                inner.login = Login::PasswordNeeded(password_token);
                Ok(UiState::NeedPassword { hint })
            }
            Err(err) => {
                let err = UiError::from(err);
                // An expired code cannot be retried; anything else can.
                if err.code != "PHONE_CODE_EXPIRED" {
                    inner.login = Login::CodeSent { token, phone };
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
            Login::CodeSent { phone, .. } => {
                return Ok(UiState::NeedCode {
                    phone: phone.clone(),
                });
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
        // A new sign-in: its authorization key goes to disk at once.
        if let Some(key) = self.key() {
            seal_db(&connection.db, &key, &self.data_dir.join(DB_SEALED), None);
        }
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

            let flusher = self.start_flusher(&db);
            inner.connection = Some(Connection {
                client: Client::new(handle),
                session,
                db,
                pool_task: tokio::spawn(runner.run()),
                updates: Some(updates),
                update_task: None,
                me: None,
                flusher,
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
        let db = match self.key() {
            // A passcode: the database lives in memory, sealed on disk.
            Some(key) => {
                let image = vault::read_sealed(&self.data_dir.join(DB_SEALED), &key).map_err(vault_error)?;
                store::open_memory(image)?
            }
            None => store::open(self.data_dir.join(DB_FILE))?,
        };
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
        if let Some(task) = connection.flusher {
            task.abort();
        }
        connection.client.disconnect();
        connection.pool_task.abort();
        // Wait until the runner has really stopped using the session.
        let _ = connection.pool_task.await;
        if let Some(key) = self.key() {
            seal_db(&connection.db, &key, &self.data_dir.join(DB_SEALED), None);
        }
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
        if let Ok(mut slot) = self.key.lock() {
            *slot = key.map(Arc::new);
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

    /// Seals the in-memory database now (at exit, and by the app's timer).
    pub async fn flush(&self) {
        let Some(key) = self.key() else { return };
        let inner = self.inner.lock().await;
        if let Some(connection) = &inner.connection {
            seal_db(&connection.db, &key, &self.data_dir.join(DB_SEALED), None);
        }
    }

    /// Starts the background sealing of a passcode-protected database.
    fn start_flusher(&self, db: &Db) -> Option<JoinHandle<()>> {
        let key = self.key()?;
        let db = Arc::clone(db);
        let path = self.data_dir.join(DB_SEALED);
        Some(tokio::spawn(async move {
            let last = std::sync::atomic::AtomicU64::new(u64::MAX);
            loop {
                tokio::time::sleep(FLUSH_EVERY).await;
                seal_db(&db, &key, &path, Some(&last));
            }
        }))
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
        self.set_key(Some(key));
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
        self.clear_cache();
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
        let plain_db = self.data_dir.join(DB_FILE);
        let image = store::image_of_file(&plain_db)?;
        std::fs::create_dir_all(&self.data_dir)?;
        if let Some(image) = image {
            vault::write_sealed(&self.data_dir.join(DB_SEALED), &key, &image)?;
        }
        if let Some(keys) = &inner.keys {
            let raw = serde_json::to_vec(keys).map_err(UiError::internal)?;
            vault::write_sealed(&self.config_dir.join(KEYS_SEALED), &key, &raw)?;
        }
        // The vault last: until it exists, the plain files still rule.
        vault::save(&self.config_dir, &file)?;
        for name in [DB_FILE, "oxigram.db-wal", "oxigram.db-shm"] {
            vault::shred(&self.data_dir.join(name));
        }
        vault::shred(&self.config_dir.join("config.json"));
        self.clear_cache();
        self.set_key(Some(key));
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
        if let Some(image) = vault::read_sealed(&self.data_dir.join(DB_SEALED), &key).map_err(vault_error)? {
            store::write_file(image, &self.data_dir.join(DB_FILE))?;
        }
        if let Some(keys) = self.sealed_keys(&key)? {
            config::save_keys(&self.config_dir, &keys)?;
        }
        vault::shred(&vault::path(&self.config_dir));
        vault::shred(&self.data_dir.join(DB_SEALED));
        vault::shred(&self.config_dir.join(KEYS_SEALED));
        self.set_key(None);
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
        let db_path = self.data_dir.join(DB_SEALED);
        let keys_path = self.config_dir.join(KEYS_SEALED);
        let image = vault::read_sealed(&db_path, &old_key).map_err(vault_error)?;
        let keys = vault::read_sealed(&keys_path, &old_key).map_err(vault_error)?;
        if let Some(image) = image {
            vault::write_sealed(&db_path, &key, &image)?;
        }
        if let Some(keys) = keys {
            vault::write_sealed(&keys_path, &key, &keys)?;
        }
        vault::save(&self.config_dir, &file)?;
        self.set_key(Some(key));
        Ok(())
    }

    /// "Забыли код?": the encrypted data cannot be opened, so it is deleted
    /// (the account is signed out locally; its session stays listed in
    /// Telegram's active sessions until ended from another device).
    pub async fn forget_passcode(&self) -> Result<UiState, UiError> {
        let mut inner = self.inner.lock().await;
        self.set_key(None);
        self.disconnect(&mut inner).await;
        inner.keys = None;
        for path in [
            vault::path(&self.config_dir),
            self.config_dir.join(KEYS_SEALED),
            self.data_dir.join(DB_SEALED),
            self.data_dir.join(DB_FILE),
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

    /// The passcode moves the data between plain and sealed files.
    #[tokio::test(flavor = "multi_thread")]
    async fn passcode_seals_and_restores_the_data() {
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
            let conn = db.lock().unwrap();
            conn.execute_batch("CREATE TABLE marker (v TEXT); INSERT INTO marker VALUES ('very-secret-text');").unwrap();
        }

        // On: readable files go, sealed ones come, media is removed.
        let telegram = Telegram::new(config_dir.clone(), data_dir.clone(), cache_dir.clone());
        telegram.enable_passcode("1234").await.unwrap();
        assert!(!data_dir.join(DB_FILE).exists());
        assert!(!config_dir.join("config.json").exists());
        assert!(!cache_dir.join("avatars").join("a.jpg").exists());
        let sealed = std::fs::read(data_dir.join(DB_SEALED)).unwrap();
        assert!(!sealed.windows(16).any(|w| w == b"very-secret-text"));
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

        // A new passcode; the old one no longer opens.
        telegram.change_passcode("1234", "abcd").await.unwrap();
        assert!(telegram.verify("1234").await.is_err());

        // Off: the plain files are back with their contents.
        telegram.disable_passcode("abcd").await.unwrap();
        assert!(!vault::path(&config_dir).exists());
        assert!(!data_dir.join(DB_SEALED).exists());
        assert!(config::load_keys(&config_dir) == Some(keys));
        let db = store::open(data_dir.join(DB_FILE)).unwrap();
        let value: String = db.lock().unwrap().query_row("SELECT v FROM marker", [], |r| r.get(0)).unwrap();
        assert_eq!(value, "very-secret-text");
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
