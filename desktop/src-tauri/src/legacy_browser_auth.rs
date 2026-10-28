//! Explicit cleanup for pre-0.1.3 managed-device sign-ins. Never used for human login.
use crate::Session;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;
#[derive(Clone, Serialize, Deserialize)]
struct Enrollment {
    origin: String,
    request_id: Uuid,
    device_id: Uuid,
    pub(crate) credential: String,
    poll_secret: String,
    principal_id: Option<Uuid>,
    pending_logout: bool,
}
fn hash(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}
fn valid_record(e: &Enrollment, origin: &str) -> bool {
    e.origin == origin
        && !e.request_id.is_nil()
        && !e.device_id.is_nil()
        && e.principal_id.is_none_or(|id| !id.is_nil())
        && e.credential.len() == 68
        && e.credential.starts_with("mlt_")
        && e.credential.as_bytes()[4..]
            .iter()
            .all(u8::is_ascii_hexdigit)
        && e.poll_secret.len() == 64
        && e.poll_secret.bytes().all(|b| b.is_ascii_hexdigit())
}
// Synchronous Secret Service/Keychain calls run off the async executor. All
// callers serialize them with BrowserAuth, including reconnect and revocation.
async fn store(origin: &str, operation: Store) -> Result<Option<Enrollment>, String> {
    let origin = origin.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let entry = keyring::Entry::new("com.codifix.desktop.browser", &hash(&origin))
            .map_err(|_| "Cannot access the desktop keyring")?;
        match operation {
            Store::Read => match entry.get_password() {
                Ok(value) => {
                    let e: Enrollment = serde_json::from_str(&value).map_err(|_| "Invalid saved desktop sign-in")?;
                    if !valid_record(&e, &origin) { return Err("Saved sign-in origin or identity mismatch".into()); }
                    Ok(Some(e))
                }
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err("Unlock your desktop keyring and try again. No plaintext credential fallback is used.".into()),
            },
            Store::Write(e) => {
                let value = serde_json::to_string(&e).map_err(|_| "Cannot encode desktop sign-in")?;
                entry.set_password(&value).map_err(|_| "Cannot save sign-in in the desktop keyring; unlock it and retry")?;
                Ok(None)
            }
            Store::Delete => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err("Could not remove the saved sign-in from the desktop keyring; unlock it and retry.".into()),
            },
        }
    }).await.map_err(|_| "Desktop keyring operation failed")?
}
enum Store {
    Read,
    Write(Enrollment),
    Delete,
}
async fn revoke(session: &Session, e: &Enrollment) -> Result<(), String> {
    let result = session
        .request(
            "/v1/onboarding/device/cancel",
            Method::POST,
            Some(json!({"request_id":e.request_id,"poll_secret":e.poll_secret})),
        )
        .await;
    match result {
        Ok(receipt) if receipt["status"] == "denied" && receipt["request_id"] == e.request_id.to_string() && receipt["device_id"] == e.device_id.to_string() => Ok(()),
        Err(error) if error.contains("HTTP 409.") => {
            let mut next = Session::new(session.endpoint.clone(), Some(e.credential.clone()))?;
            next.area = session.area.clone();
            let session = next;
            let receipt = session.request("/v1/onboarding/device/logout", Method::POST, Some(json!({}))).await?;
            if receipt["logged_out"] != true || receipt["device_id"] != e.device_id.to_string() { return Err("Sign-out receipt identity mismatch; saved sign-in retained".into()); }
            Ok(())
        }
        Err(error) => Err(format!("{error} Sign-out is unconfirmed; saved sign-in retained. Retry Sign out browser access.")),
        _ => Err("Cancellation receipt mismatch; saved sign-in retained".into()),
    }
}

pub(crate) async fn cleanup(
    origin: &Url,
    binding: Option<crate::area_routing::Binding>,
) -> Result<(), String> {
    let name = origin.origin().ascii_serialization();
    if let Some(mut saved) = store(&name, Store::Read).await? {
        saved.pending_logout = true;
        store(&name, Store::Write(saved.clone())).await?;
        let mut session = Session::new(origin.clone(), None)?;
        session.area = binding;
        revoke(&session, &saved).await?;
        store(&name, Store::Delete).await?;
    }
    Ok(())
}
