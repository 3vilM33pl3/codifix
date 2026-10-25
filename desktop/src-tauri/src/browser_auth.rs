//! Browser-approved device credentials never cross the renderer boundary.
use crate::{Session, Transport};
use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use tauri::State;
use url::Url;
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct BrowserAuth(pub tokio::sync::Mutex<()>);
// No Debug: neither secrets nor remote response bodies belong in diagnostics.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Enrollment {
    origin: String,
    request_id: Uuid,
    device_id: Uuid,
    pub(crate) credential: String,
    poll_secret: String,
    principal_id: Option<Uuid>,
    pending_logout: bool,
}
#[derive(Clone)]
pub(crate) struct Pending {
    enrollment: Enrollment,
    session: Session,
    generation: u64,
    info: Progress,
    deadline: Instant,
    next_poll: Instant,
}
#[derive(Clone, Serialize)]
pub(crate) struct Progress {
    status: String,
    request_id: Uuid,
    device_id: Uuid,
    credential_sha256: String,
    user_code: String,
    verification_uri: String,
    expires_at: DateTime<Utc>,
    auth: Option<Value>,
}
use codifix_record::{
    AuthRole, OnboardingArea, OnboardingDevicePollResponse,
    OnboardingDeviceStartResponse as Started,
};
fn hash(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}
fn fresh(origin: &Url) -> Enrollment {
    Enrollment {
        origin: origin.origin().ascii_serialization(),
        request_id: Uuid::new_v4(),
        device_id: Uuid::new_v4(),
        credential: format!("mlt_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
        poll_secret: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
        principal_id: None,
        pending_logout: false,
    }
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
pub(crate) async fn restore(origin: &Url) -> Result<Option<Enrollment>, String> {
    let e = store(&origin.origin().ascii_serialization(), Store::Read).await?;
    if e.as_ref().is_some_and(|e| e.pending_logout) {
        return Err(
            "A previous sign-out is unconfirmed. Use Sign out browser access to finish it.".into(),
        );
    }
    Ok(e.filter(|e| e.principal_id.is_some()))
}
pub(crate) fn validate_auth(auth: &Value, e: &Enrollment) -> Result<(), String> {
    if auth["authenticated"] != true
        || auth["mode"] != "multi_user"
        || auth["principal"]["kind"] != "service_token"
        || auth["principal"]["id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            != e.principal_id
        || e.principal_id.is_none()
    {
        return Err("Desktop sign-in identity could not be verified".into());
    }
    Ok(())
}
fn validate_start(start: Started, e: &Enrollment) -> Result<(Progress, Duration), String> {
    let code = &start.user_code;
    if start.request_id != e.request_id
        || start.poll_interval_seconds != 2
        || code.len() != 9
        || code.as_bytes()[4] != b'-'
        || !code
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 4 || b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return Err("Browser sign-in receipt did not match this device".into());
    }
    let expected = format!("{}/?device_code={code}", e.origin);
    if start.verification_uri_complete != expected
        && start.verification_uri_complete != format!("{expected}&company_device=1")
    {
        return Err(
            "Browser approval URL did not match the selected service. Use its public HTTPS origin."
                .into(),
        );
    }
    let remaining = (start.expires_at - Utc::now())
        .to_std()
        .unwrap_or_default()
        .min(Duration::from_secs(600));
    if remaining.is_zero() {
        return Err("This sign-in request expired. Cancel it before starting again.".into());
    }
    Ok((
        Progress {
            status: "pending".into(),
            request_id: e.request_id,
            device_id: e.device_id,
            credential_sha256: hash(&e.credential),
            user_code: start.user_code,
            verification_uri: start.verification_uri_complete,
            expires_at: start.expires_at,
            auth: None,
        },
        remaining,
    ))
}
fn validate_approval(receipt: &Value, e: &mut Enrollment) -> Result<(), String> {
    let receipt: OnboardingDevicePollResponse =
        serde_json::from_value(receipt.clone()).map_err(|_| "Invalid browser approval contract")?;
    if receipt.status != "approved"
        || receipt.device_id != Some(e.device_id)
        || receipt.principal_id.is_none_or(|id| id.is_nil())
        || receipt.projects.as_ref().is_none_or(|p| {
            p.is_empty()
                || p.iter().any(|p| {
                    !matches!(p.role, AuthRole::Reader | AuthRole::Writer)
                        || p.manifest.project_id.is_nil()
                        || p.manifest.entry.owner_instance_id.is_nil()
                        || (p.manifest.entry.area == OnboardingArea::Trust
                            && p.manifest.entry.offline_allowed)
                })
        })
    {
        return Err("Browser approval identity or permissions did not match this device".into());
    }
    e.principal_id = receipt.principal_id;
    Ok(())
}

fn current(state: &Transport, generation: u64) -> Result<(), String> {
    if state
        .0
        .lock()
        .map_err(|_| "Connection unavailable")?
        .generation
        != generation
    {
        return Err("Connection changed; sign-in was not activated".into());
    }
    Ok(())
}
#[tauri::command]
pub(crate) async fn start_browser_login(
    endpoint: String,
    state: State<'_, Transport>,
    browser: State<'_, BrowserAuth>,
) -> Result<Progress, String> {
    let origin = crate::endpoint(&endpoint)?;
    if origin.scheme() != "https" {
        return Err("Browser sign-in requires your Codifix public HTTPS service address. Local single-user services use Connect locally below.".into());
    }
    let generation = {
        let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
        c.generation += 1;
        c.session = None;
        c.pending = None;
        c.generation
    };
    let _operation = browser.0.lock().await;
    current(&state, generation)?;
    let origin_text = origin.origin().ascii_serialization();
    let e = match store(&origin_text, Store::Read).await? {
        Some(e) => e,
        None => {
            let e = fresh(&origin);
            store(&origin_text, Store::Write(e.clone())).await?;
            e
        }
    };
    if e.pending_logout {
        return Err(
            "A previous sign-out is unconfirmed. Use Sign out browser access to finish it.".into(),
        );
    }
    if e.principal_id.is_some() {
        let session = Session::new(origin, Some(e.credential.clone()))?;
        let auth = session.request("/v1/auth/me", Method::GET, None).await?;
        validate_auth(&auth, &e)?;
        let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
        if c.generation != generation {
            return Err("Connection changed; sign-in was not activated".into());
        }
        c.session = Some(session);
        return Ok(Progress {
            status: "approved".into(),
            request_id: e.request_id,
            device_id: e.device_id,
            credential_sha256: hash(&e.credential),
            user_code: String::new(),
            verification_uri: String::new(),
            expires_at: Utc::now(),
            auth: Some(auth),
        });
    }
    let session = Session::new(origin, None)?;
    let value = session.request("/v1/onboarding/device/start", Method::POST, Some(json!({
        "request_id": e.request_id, "device_id": e.device_id,
        "name": "Codifix Desktop", "platform": std::env::consts::OS,
        "credential_sha256": hash(&e.credential), "poll_sha256": hash(&e.poll_secret)
    }))).await.map_err(|error| format!("{error} Browser sign-in needs a service with device onboarding enabled. The request is saved; retry or cancel it."))?;
    let started: Started =
        serde_json::from_value(value).map_err(|_| "Invalid browser sign-in receipt")?;
    let (info, remaining) = validate_start(started, &e)?;
    let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
    if c.generation != generation {
        return Err("Connection changed; sign-in was not activated".into());
    }
    c.pending = Some(Pending {
        enrollment: e,
        session,
        generation,
        info: info.clone(),
        deadline: Instant::now() + remaining,
        next_poll: Instant::now(),
    });
    Ok(info)
}
#[tauri::command]
pub(crate) fn open_browser_login(
    request_id: String,
    state: State<'_, Transport>,
) -> Result<(), String> {
    let c = state.0.lock().map_err(|_| "Connection unavailable")?;
    let p = c
        .pending
        .as_ref()
        .filter(|p| {
            p.generation == c.generation
                && p.enrollment.request_id.to_string() == request_id
                && Instant::now() < p.deadline
        })
        .ok_or("No current browser sign-in request")?;
    open::that(&p.info.verification_uri).map_err(|_| {
        "Could not open your browser. Open the displayed approval URL manually.".into()
    })
}
#[tauri::command]
pub(crate) async fn poll_browser_login(
    request_id: String,
    state: State<'_, Transport>,
    browser: State<'_, BrowserAuth>,
) -> Result<Progress, String> {
    let _operation = browser.0.lock().await;
    let mut p = {
        let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
        let generation = c.generation;
        let p = c
            .pending
            .as_mut()
            .filter(|p| {
                p.generation == generation && p.enrollment.request_id.to_string() == request_id
            })
            .ok_or("No current browser sign-in request")?;
        if Instant::now() >= p.deadline {
            return Err("Browser sign-in expired. Cancel this request and try again.".into());
        }
        if Instant::now() < p.next_poll {
            return Ok(p.info.clone());
        }
        p.next_poll = Instant::now() + Duration::from_secs(3);
        p.clone()
    };
    let receipt = p.session.request("/v1/onboarding/device/poll", Method::POST, Some(json!({"request_id":p.enrollment.request_id,"poll_secret":p.enrollment.poll_secret}))).await?;
    current(&state, p.generation)?;
    if Instant::now() >= p.deadline {
        return Err("Browser sign-in expired. Cancel this request and try again.".into());
    }
    match receipt["status"].as_str() {
        Some("pending") => Ok(p.info),
        Some("approved") => {
            validate_approval(&receipt, &mut p.enrollment)?;
            let session = Session::new(p.session.endpoint, Some(p.enrollment.credential.clone()))?;
            let auth = session.request("/v1/auth/me", Method::GET, None).await?;
            validate_auth(&auth, &p.enrollment)?;
            // Save before activation, so crash/cancellation can reconcile issuance.
            store(&p.enrollment.origin, Store::Write(p.enrollment.clone())).await?;
            let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
            if c.generation != p.generation {
                return Err("Connection changed; sign-in was not activated".into());
            }
            c.session = Some(session);
            c.pending = None;
            p.info.status = "approved".into();
            p.info.auth = Some(auth);
            Ok(p.info)
        }
        Some("denied" | "expired") => Err(
            "Browser sign-in was denied or expired. Cancel this request before trying again."
                .into(),
        ),
        _ => Err("Unrecognized browser sign-in status; no session activated".into()),
    }
}
#[tauri::command]
pub(crate) async fn cancel_browser_login(
    endpoint: String,
    state: State<'_, Transport>,
    browser: State<'_, BrowserAuth>,
) -> Result<(), String> {
    let origin = crate::endpoint(&endpoint)?;
    {
        let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
        c.generation += 1;
        c.session = None;
        c.pending = None;
    }
    let _operation = browser.0.lock().await;
    let origin_text = origin.origin().ascii_serialization();
    let Some(mut e) = store(&origin_text, Store::Read).await? else {
        return Ok(());
    };
    e.pending_logout = true;
    store(&origin_text, Store::Write(e.clone())).await?;
    let session = Session::new(origin, None)?;
    revoke(&session, &e).await?;
    store(&origin_text, Store::Delete).await?;
    Ok(())
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
            let session = Session::new(session.endpoint.clone(), Some(e.credential.clone()))?;
            let receipt = session.request("/v1/onboarding/device/logout", Method::POST, Some(json!({}))).await?;
            if receipt["logged_out"] != true || receipt["device_id"] != e.device_id.to_string() { return Err("Sign-out receipt identity mismatch; saved sign-in retained".into()); }
            Ok(())
        }
        Err(error) => Err(format!("{error} Sign-out is unconfirmed; saved sign-in retained. Retry Sign out browser access.")),
        _ => Err("Cancellation receipt mismatch; saved sign-in retained".into()),
    }
}

/// Explicit recovery only: forgetting an unreachable enrollment is not revocation.
#[tauri::command]
pub(crate) async fn forget_browser_login(
    endpoint: String,
    confirmed: bool,
    state: State<'_, Transport>,
    browser: State<'_, BrowserAuth>,
) -> Result<(), String> {
    if !confirmed {
        return Err("Confirm that forgetting locally does not revoke server access".into());
    }
    let origin = crate::endpoint(&endpoint)?.origin().ascii_serialization();
    {
        let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
        c.generation += 1;
        c.session = None;
        c.pending = None;
    }
    let _operation = browser.0.lock().await;
    store(&origin, Store::Delete).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn started(e: &Enrollment) -> Started {
        Started {
            request_id: e.request_id,
            user_code: "ABCD-1234".into(),
            verification_uri_complete: format!("{}/?device_code=ABCD-1234", e.origin),
            expires_at: Utc::now() + chrono::Duration::minutes(10),
            poll_interval_seconds: 2,
        }
    }
    fn approval(e: &Enrollment) -> Value {
        json!({"status":"approved", "device_id":e.device_id,"principal_id":Uuid::new_v4(),"projects":[{"role":"writer","manifest":{"project_id":Uuid::new_v4(),"project":"demo","area":"workshop","owner_instance_id":Uuid::new_v4(),"repository_url":"https://example.com/demo","default_branch":"main","offline_allowed":true,"tools":[],"destinations":[],"revision":1,"reviewed_by":Uuid::new_v4(),"reviewed_at":Utc::now()}}]})
    }
    #[test]
    fn binds_approval_url_and_start_receipt_to_origin_and_request() {
        let e = fresh(&Url::parse("https://company.example").unwrap());
        assert!(validate_start(started(&e), &e).is_ok());
        let mut company = started(&e);
        company
            .verification_uri_complete
            .push_str("&company_device=1");
        assert!(validate_start(company, &e).is_ok());
        for url in [
            "https://evil.example/?device_code=ABCD-1234",
            "https://company.example/?device_code=ABCD-1234&token=secret",
            "file:///tmp/program",
            "https://company.example/v1/auth/login",
        ] {
            let mut s = started(&e);
            s.verification_uri_complete = url.into();
            assert!(validate_start(s, &e).is_err());
        }
        let mut s = started(&e);
        s.request_id = Uuid::new_v4();
        assert!(validate_start(s, &e).is_err());
        let mut s = started(&e);
        s.expires_at = Utc::now() - chrono::Duration::seconds(1);
        assert!(validate_start(s, &e).is_err());
        let mut s = started(&e);
        s.user_code = "ABCD/1234".into();
        assert!(validate_start(s, &e).is_err());
    }
    #[test]
    fn checks_device_principal_and_bounded_grants_before_activation() {
        let mut e = fresh(&Url::parse("https://company.example").unwrap());
        let receipt = approval(&e);
        validate_approval(&receipt, &mut e).unwrap();
        let auth = json!({"authenticated":true,"mode":"multi_user","principal":{"id":e.principal_id,"kind":"service_token"}});
        validate_auth(&auth, &e).unwrap();
        let mut wrong = auth.clone();
        wrong["principal"]["id"] = json!(Uuid::new_v4());
        assert!(validate_auth(&wrong, &e).is_err());
        let mut wrong = auth;
        wrong["authenticated"] = json!(false);
        assert!(validate_auth(&wrong, &e).is_err());
        for field in ["device", "principal", "role", "trust", "empty"] {
            let mut wrong = receipt.clone();
            match field {
                "device" => wrong["device_id"] = json!(Uuid::new_v4()),
                "principal" => wrong["principal_id"] = json!(Uuid::nil()),
                "role" => wrong["projects"][0]["role"] = json!("admin"),
                "trust" => wrong["projects"][0]["manifest"]["area"] = json!("trust"),
                _ => wrong["projects"] = json!([]),
            }
            assert!(validate_approval(&wrong, &mut e).is_err(), "{field}");
        }
    }
    #[test]
    fn changed_connection_fences_late_approval_and_saved_origin_is_bound() {
        let state = Transport::default();
        assert!(current(&state, 0).is_ok());
        state.0.lock().unwrap().generation += 1;
        assert!(current(&state, 0).is_err());
        let e = fresh(&Url::parse("https://company.example").unwrap());
        assert!(valid_record(&e, "https://company.example"));
        assert!(!valid_record(&e, "https://other.example"));
        assert_ne!(hash(&e.credential), hash(&e.poll_secret));
        let (public, _) = validate_start(started(&e), &e).unwrap();
        let public = serde_json::to_string(&public).unwrap();
        assert!(!public.contains(&e.credential));
        assert!(!public.contains(&e.poll_secret));
    }
    // Isolated HTTP server checks the approve/cancel race and its exact proof.
    #[test]
    fn cancellation_reconciles_already_approved_device_without_following_redirects() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let e = fresh(&origin);
        let server_e = e.clone();
        let server = std::thread::spawn(move || {
            for index in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut chunk = [0; 4096];
                    let n = socket.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&bytes);
                    if let Some((header, body)) = text.split_once("\r\n\r\n") {
                        let len: usize = header
                            .lines()
                            .find_map(|s| {
                                s.to_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|s| s.parse().ok())
                            })
                            .unwrap_or(0);
                        if body.len() >= len {
                            break;
                        }
                    }
                }
                let text = String::from_utf8(bytes).unwrap();
                if index == 0 {
                    assert!(text.starts_with("POST /v1/onboarding/device/cancel "));
                    let body: Value =
                        serde_json::from_str(text.split_once("\r\n\r\n").unwrap().1).unwrap();
                    assert_eq!(body["request_id"], server_e.request_id.to_string());
                    assert_eq!(body["poll_secret"], server_e.poll_secret);
                    assert!(!text.to_lowercase().contains("authorization:"));
                } else {
                    assert!(text.starts_with("POST /v1/onboarding/device/logout "));
                    assert!(text.contains(&format!("Bearer {}", server_e.credential)));
                }
                let body = if index == 0 {
                    "{}".into()
                } else {
                    json!({"logged_out":true,"device_id":server_e.device_id}).to_string()
                };
                write!(
                    socket,
                    "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    if index == 0 { "409 Conflict" } else { "200 OK" },
                    body.len(),
                    body
                )
                .unwrap();
            }
        });
        tauri::async_runtime::block_on(async {
            revoke(&Session::new(origin, None).unwrap(), &e)
                .await
                .unwrap();
        });
        server.join().unwrap();
    }
    #[test]
    #[ignore = "uses a disposable entry in the current desktop keyring"]
    fn host_keyring_roundtrip() {
        tauri::async_runtime::block_on(async {
            let origin = Url::parse(&format!("https://{}.invalid", Uuid::new_v4())).unwrap();
            let e = fresh(&origin);
            store(&e.origin, Store::Write(e.clone())).await.unwrap();
            let read = store(&e.origin, Store::Read).await.unwrap().unwrap();
            let same = read.credential == e.credential && read.request_id == e.request_id;
            store(&e.origin, Store::Delete).await.unwrap();
            assert!(same);
            assert!(store(&e.origin, Store::Read).await.unwrap().is_none());
        });
    }
}
