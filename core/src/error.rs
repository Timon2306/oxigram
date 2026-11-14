//! The single error shape that crosses the IPC boundary.

use grammers_client::{InvocationError, SignInError};
use serde::Serialize;

/// Sent to the frontend as the rejection value of a command.
///
/// `code` is stable and machine-readable (Telegram's RPC error name where there
/// is one); the frontend maps it to a localized text and falls back to `message`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiError {
    pub code: String,
    pub message: String,
    /// Set for flood waits: how long Telegram asks to wait before retrying.
    pub wait_seconds: Option<u32>,
}

impl UiError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            wait_seconds: None,
        }
    }

    pub fn internal(err: impl std::fmt::Display) -> Self {
        Self::new("INTERNAL", err.to_string())
    }
}

impl From<InvocationError> for UiError {
    fn from(err: InvocationError) -> Self {
        match &err {
            InvocationError::Rpc(rpc) => {
                let is_wait = matches!(
                    rpc.name.as_str(),
                    "FLOOD_WAIT" | "FLOOD_PREMIUM_WAIT" | "SLOWMODE_WAIT"
                );
                Self {
                    code: rpc.name.clone(),
                    message: err.to_string(),
                    wait_seconds: if is_wait { rpc.value } else { None },
                }
            }
            InvocationError::Io(_)
            | InvocationError::Transport(_)
            | InvocationError::Dropped(_)
            | InvocationError::Authentication(_) => Self::new("NETWORK", err.to_string()),
            _ => Self::internal(err),
        }
    }
}

impl From<SignInError> for UiError {
    fn from(err: SignInError) -> Self {
        match err {
            SignInError::SignUpRequired => Self::new("SIGN_UP_REQUIRED", "sign up required"),
            SignInError::InvalidCode => Self::new("PHONE_CODE_INVALID", "invalid code"),
            SignInError::InvalidPassword(_) => Self::new("PASSWORD_INVALID", "invalid password"),
            SignInError::PasswordRequired(_) => {
                Self::new("SESSION_PASSWORD_NEEDED", "2FA password required")
            }
            SignInError::Other(err) => err.into(),
        }
    }
}

impl From<rusqlite::Error> for UiError {
    fn from(err: rusqlite::Error) -> Self {
        Self::new("STORAGE", err.to_string())
    }
}

impl From<std::io::Error> for UiError {
    fn from(err: std::io::Error) -> Self {
        Self::new("STORAGE", err.to_string())
    }
}
