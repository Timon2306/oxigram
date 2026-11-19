//! Signing in by code and by QR, with Telegram's own requests: grammers'
//! `request_login_code` drops how the code was sent (app, SMS, call…) and
//! offers no resend or QR login.

use grammers_client::{Client, InvocationError};
use grammers_client::tl;
use grammers_session::Session;
use grammers_session::types::{PeerInfo, UpdateState, UpdatesState};
use serde::Serialize;

use crate::error::UiError;
use crate::store::session::SqliteSession;

/// How the code went out, for the code screen.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodeSent {
    pub phone: String,
    /// "app", "sms", "call", "flash_call", "missed_call", "email",
    /// "email_setup", "fragment", "firebase", "sms_word", "sms_phrase".
    pub via: String,
    /// What "send the code another way" would use: "sms", "call",
    /// "flash_call", "missed_call", "fragment"; `None` when there is nothing else.
    pub next: Option<String>,
    /// Seconds before the other way may be asked for.
    pub timeout: Option<i32>,
    /// Digits in the code, when known.
    pub length: Option<i32>,
    /// The e-mail pattern ("a***@g***.com") or the Fragment link.
    pub hint: Option<String>,
}

fn sent(phone: &str, code: tl::types::auth::SentCode) -> (String, CodeSent) {
    use tl::enums::auth::SentCodeType as T;
    let (via, length, hint) = match code.r#type {
        T::App(t) => ("app", Some(t.length), None),
        T::Sms(t) => ("sms", Some(t.length), None),
        T::Call(t) => ("call", Some(t.length), None),
        T::FlashCall(_) => ("flash_call", None, None),
        T::MissedCall(t) => ("missed_call", Some(t.length), Some(t.prefix)),
        T::EmailCode(t) => ("email", Some(t.length), Some(t.email_pattern)),
        T::SetUpEmailRequired(_) => ("email_setup", None, None),
        T::FragmentSms(t) => ("fragment", Some(t.length), Some(t.url)),
        T::FirebaseSms(t) => ("firebase", Some(t.length), None),
        T::SmsWord(_) => ("sms_word", None, None),
        T::SmsPhrase(_) => ("sms_phrase", None, None),
    };
    let next = code.next_type.map(|next| {
        use tl::enums::auth::CodeType as C;
        match next {
            C::Sms => "sms",
            C::Call => "call",
            C::FlashCall => "flash_call",
            C::MissedCall => "missed_call",
            C::FragmentSms => "fragment",
        }
        .to_owned()
    });
    let info = CodeSent {
        phone: phone.to_owned(),
        via: via.to_owned(),
        next,
        timeout: code.timeout,
        length,
        hint,
    };
    (code.phone_code_hash, info)
}

/// A request on the home data centre, moving there first when Telegram
/// answers that the account lives elsewhere (PHONE_MIGRATE_X and the like).
async fn invoke_home<R: tl::RemoteCall>(
    client: &Client,
    session: &SqliteSession,
    request: &R,
) -> Result<R::Return, InvocationError> {
    match client.invoke(request).await {
        Err(InvocationError::Rpc(err)) if err.code == 303 => {
            if let Some(dc) = err.value {
                let _ = session.set_home_dc_id(dc as i32).await;
            }
            client.invoke(request).await
        }
        other => other,
    }
}

fn unpack(sent: tl::enums::auth::SentCode) -> Result<tl::types::auth::SentCode, UiError> {
    match sent {
        tl::enums::auth::SentCode::Code(code) => Ok(code),
        _ => Err(UiError::new("SEND_CODE_UNSUPPORTED", "this sign-in needs the official app")),
    }
}

/// auth.sendCode: the phone code hash and how the code went out.
pub async fn send_code(
    client: &Client,
    session: &SqliteSession,
    phone: &str,
    api_id: i32,
    api_hash: &str,
) -> Result<(String, CodeSent), UiError> {
    let request = tl::functions::auth::SendCode {
        phone_number: phone.to_owned(),
        api_id,
        api_hash: api_hash.to_owned(),
        settings: tl::types::CodeSettings {
            allow_flashcall: false,
            current_number: false,
            allow_app_hash: false,
            allow_missed_call: false,
            allow_firebase: false,
            logout_tokens: None,
            token: None,
            app_sandbox: None,
            unknown_number: false,
        }
        .into(),
    };
    let code = unpack(invoke_home(client, session, &request).await?)?;
    Ok(sent(phone, code))
}

/// auth.resendCode: the code another way (the previous answer's `next`).
pub async fn resend_code(client: &Client, phone: &str, hash: &str) -> Result<(String, CodeSent), UiError> {
    let request = tl::functions::auth::ResendCode {
        phone_number: phone.to_owned(),
        phone_code_hash: hash.to_owned(),
        reason: None,
    };
    let code = unpack(client.invoke(&request).await?)?;
    Ok(sent(phone, code))
}

/// What a sign-in step ended in.
pub enum Signed {
    In,
    /// Two-step verification: the account's password settings.
    Password(grammers_client::client::PasswordToken),
}

/// After an authorization: what grammers' own sign-in records (the user as
/// "self", the update state).
async fn complete(client: &Client, session: &SqliteSession, auth: tl::enums::auth::Authorization) -> Result<Signed, UiError> {
    let tl::enums::auth::Authorization::Authorization(auth) = auth else {
        return Err(UiError::new("SIGN_UP_REQUIRED", "sign up required"));
    };
    let state = client.invoke(&tl::functions::updates::GetState {}).await.ok();
    session.cache_peer(PeerInfo::from(auth.user)).await.map_err(UiError::internal)?;
    if let Some(tl::enums::updates::State::State(state)) = state {
        session
            .set_update_state(UpdateState::All(UpdatesState {
                pts: state.pts,
                qts: state.qts,
                date: state.date,
                seq: state.seq,
                channels: Vec::new(),
            }))
            .await
            .map_err(UiError::internal)?;
    }
    Ok(Signed::In)
}

/// SESSION_PASSWORD_NEEDED: the password step.
async fn password(client: &Client) -> Result<Signed, UiError> {
    let tl::enums::account::Password::Password(password) = client.invoke(&tl::functions::account::GetPassword {}).await?;
    Ok(Signed::Password(grammers_client::client::PasswordToken::new(password)))
}

/// auth.signIn with the code.
pub async fn sign_in(client: &Client, session: &SqliteSession, phone: &str, hash: &str, code: &str) -> Result<Signed, UiError> {
    let request = tl::functions::auth::SignIn {
        phone_number: phone.to_owned(),
        phone_code_hash: hash.to_owned(),
        phone_code: Some(code.to_owned()),
        email_verification: None,
    };
    match client.invoke(&request).await {
        Ok(auth) => complete(client, session, auth).await,
        Err(err) if err.is("SESSION_PASSWORD_NEEDED") => password(client).await,
        Err(err) => Err(err.into()),
    }
}

/// The QR step: a link to show as a QR code, or the end of it.
pub enum Qr {
    /// tg://login?token=…; shown until `expires` (Unix seconds).
    Show { url: String, expires: i64 },
    Done(Signed),
}

fn base64url(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..=chunk.len() {
            out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// auth.exportLoginToken, asked again every few seconds while the QR code is
/// shown: a new token to show, or the sign-in done once the code was
/// scanned (moving to the account's data centre when needed).
pub async fn qr(client: &Client, session: &SqliteSession, api_id: i32, api_hash: &str) -> Result<Qr, UiError> {
    let request = tl::functions::auth::ExportLoginToken {
        api_id,
        api_hash: api_hash.to_owned(),
        except_ids: Vec::new(),
    };
    let answer = match invoke_home(client, session, &request).await {
        Ok(answer) => answer,
        Err(err) if err.is("SESSION_PASSWORD_NEEDED") => return Ok(Qr::Done(password(client).await?)),
        Err(err) => return Err(err.into()),
    };
    use tl::enums::auth::LoginToken as T;
    match answer {
        T::Token(token) => Ok(Qr::Show {
            url: format!("tg://login?token={}", base64url(&token.token)),
            expires: i64::from(token.expires),
        }),
        T::Success(success) => Ok(Qr::Done(complete(client, session, success.authorization).await?)),
        T::MigrateTo(migrate) => {
            session.set_home_dc_id(migrate.dc_id).await.map_err(UiError::internal)?;
            match client.invoke(&tl::functions::auth::ImportLoginToken { token: migrate.token }).await {
                Ok(T::Success(success)) => Ok(Qr::Done(complete(client, session, success.authorization).await?)),
                Ok(_) => Err(UiError::new("QR_FAILED", "the QR sign-in did not finish")),
                Err(err) if err.is("SESSION_PASSWORD_NEEDED") => Ok(Qr::Done(password(client).await?)),
                Err(err) => Err(err.into()),
            }
        }
    }
}

/// The dark modules of a QR code as path commands in a `size`×`size` box
/// (for Slint's Path), and that size.
pub fn qr_path(text: &str) -> Option<(String, i32)> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let width = code.width();
    let colors = code.to_colors();
    // A quiet zone of 2 modules around the code.
    let size = width + 4;
    let mut path = String::new();
    for y in 0..width {
        for x in 0..width {
            if colors[y * width + x] == qrcode::Color::Dark {
                path.push_str(&format!("M{} {}h1v1h-1z", x + 2, y + 2));
            }
        }
    }
    Some((path, size as i32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_without_padding() {
        assert_eq!(base64url(b"hi?"), "aGk_");
        assert_eq!(base64url(b"a"), "YQ");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn qr_paths() {
        let (path, size) = qr_path("tg://login?token=AAAA").unwrap();
        assert!(size >= 25);
        assert!(path.starts_with('M'));
    }
}
