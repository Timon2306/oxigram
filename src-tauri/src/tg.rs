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
use tauri::async_runtime::{self, JoinHandle};
use tokio::sync::{Mutex, Semaphore, mpsc};

use crate::config::{self, ApiKeys};
use crate::error::UiError;
use crate::store::{self, Db, session::SqliteSession};
use crate::updates::{self, UiSink};

const DB_FILE: &str = "oxigram.db";
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
}

impl Telegram {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf, cache_dir: PathBuf) -> Self {
        let keys = config::load_keys(&config_dir);
        Self {
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
        }
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
        config::save_keys(&self.config_dir, &keys)?;
        inner.keys = Some(keys);
        self.current(&mut inner).await
    }

    pub async fn clear_keys(&self) -> Result<UiState, UiError> {
        let mut inner = self.inner.lock().await;
        self.forget_account(&mut inner).await?;
        config::clear_keys(&self.config_dir)?;
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
        if let Some(receiver) = connection.updates.take() {
            connection.update_task = Some(async_runtime::spawn(updates::run(
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
                pool_task: async_runtime::spawn(runner.run()),
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
        let db = store::open(self.data_dir.join(DB_FILE))?;
        let session = SqliteSession::load(Arc::clone(&db)).map_err(UiError::internal)?;
        Ok((db, session))
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
