//! Native human login. PKCE, callback codes and credentials stay outside the renderer.
use crate::{Session, Transport};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use tauri::{Manager, State};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
};
use url::Url;
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct BrowserAuth(
    pub tokio::sync::Mutex<()>,
    std::sync::Mutex<Vec<Enrollment>>,
);
impl BrowserAuth {
    pub(crate) fn check_cleanup(&self, origin: &Url) -> Result<(), String> {
        if self
            .1
            .lock()
            .map_err(|_| "Sign-in state unavailable")?
            .iter()
            .any(|e| e.origin == origin.origin().ascii_serialization())
        {
            return Err(
                "Previous sign-out is unconfirmed. Retry Sign out before signing in again.".into(),
            );
        }
        Ok(())
    }
}
// Deliberately no Debug: secrets must never appear in diagnostics.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Enrollment {
    origin: String,
    pub(crate) credential: String,
    principal_id: Uuid,
    expires_at: DateTime<Utc>,
    pending_logout: bool,
}
pub(crate) struct Pending {
    generation: u64,
    info: Progress,
    url: Url,
    deadline: Instant,
    error: Option<String>,
    // Dropping the sender closes the listener, including on connection changes.
    _cancel: watch::Sender<()>,
}
#[derive(Clone, Serialize)]
pub(crate) struct Progress {
    status: String,
    request_id: Uuid,
    expires_at: DateTime<Utc>,
    auth: Option<Value>,
}
fn random_secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
fn hash(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}
fn valid_record(e: &Enrollment, origin: &str) -> bool {
    e.origin == origin
        && !e.principal_id.is_nil()
        && e.credential.starts_with("mld_")
        && e.credential.len() >= 36
        && e.credential.len() <= 256
        && e.credential[4..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
enum Store {
    Read,
    Write(Enrollment),
    Delete,
}
async fn store(origin: &str, operation: Store) -> Result<Option<Enrollment>, String> {
    let origin = origin.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let entry = keyring::Entry::new("com.codifix.desktop.native", &hash(&origin))
            .map_err(|_| "Cannot access the desktop keyring")?;
        match operation {
            Store::Read => match entry.get_password() {
                Ok(value) => {
                    let e: Enrollment = serde_json::from_str(&value)
                        .map_err(|_| "Invalid saved desktop sign-in")?;
                    if !valid_record(&e, &origin) {
                        return Err("Saved sign-in origin or identity mismatch".into());
                    }
                    Ok(Some(e))
                }
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err("Unlock your desktop keyring and try again.".into()),
            },
            Store::Write(e) => {
                let value =
                    serde_json::to_string(&e).map_err(|_| "Cannot encode desktop sign-in")?;
                entry
                    .set_password(&value)
                    .map_err(|_| "Cannot save sign-in; unlock your desktop keyring and retry")?;
                Ok(None)
            }
            Store::Delete => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => {
                    Err("Cannot remove saved sign-in; unlock your desktop keyring and retry".into())
                }
            },
        }
    })
    .await
    .map_err(|_| "Desktop keyring operation failed")?
}
pub(crate) async fn restore(origin: &Url) -> Result<Option<Enrollment>, String> {
    let name = origin.origin().ascii_serialization();
    let saved = store(&name, Store::Read).await?;
    if saved.as_ref().is_some_and(|e| e.pending_logout) {
        return Err("Sign-out is unconfirmed. Retry Sign out to finish it.".into());
    }
    if saved.as_ref().is_some_and(|e| e.expires_at <= Utc::now()) {
        store(&name, Store::Delete).await?;
        return Ok(None);
    }
    Ok(saved)
}
pub(crate) fn validate_auth(auth: &Value, e: &Enrollment) -> Result<(), String> {
    if auth["authenticated"] != true
        || auth["mode"] != "multi_user"
        || auth["principal"]["kind"] != "human_oidc"
        || auth["principal"]["id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            != Some(e.principal_id)
    {
        return Err("Desktop sign-in identity could not be verified".into());
    }
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
fn login_url(origin: &Url, redirect: &str, state: &str, verifier: &str) -> Result<Url, String> {
    let mut url = origin
        .join("/v1/auth/desktop/login")
        .map_err(|_| "Invalid sign-in address")?;
    url.query_pairs_mut()
        .append_pair("redirect_uri", redirect)
        .append_pair("state", state)
        .append_pair(
            "code_challenge",
            &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        )
        .append_pair("code_challenge_method", "S256");
    Ok(url)
}
#[tauri::command]
pub(crate) async fn start_browser_login(
    endpoint: String,
    app: tauri::AppHandle,
    state: State<'_, Transport>,
    browser: State<'_, BrowserAuth>,
) -> Result<Progress, String> {
    let origin = crate::endpoint(&endpoint)?;
    if origin.scheme() != "https" {
        return Err("Enter your HTTPS service address in Connection settings to sign in. Local services can connect there too.".into());
    }
    let generation = {
        let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
        c.generation += 1;
        state
            .1
            .store(c.generation, std::sync::atomic::Ordering::SeqCst);
        c.session = None;
        c.pending = None;
        c.generation
    };
    let _operation = browser.0.lock().await;
    current(&state, generation)?;
    let binding = crate::area_routing::selected(&state, &origin)?;
    crate::area_routing::verify(&origin, binding.as_ref()).await?;
    current(&state, generation)?;
    browser.check_cleanup(&origin)?;
    if let Some(saved) = restore(&origin).await? {
        let mut session = Session::new(origin.clone(), Some(saved.credential.clone()))?;
        session.area = binding.clone();
        session.fence = Some((state.1.clone(), generation));
        match session.request("/v1/auth/me", Method::GET, None).await {
            Ok(auth) => {
                validate_auth(&auth, &saved)?;
                let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
                if c.generation != generation {
                    return Err("Connection changed".into());
                }
                c.session = Some(session);
                return Ok(Progress {
                    status: "approved".into(),
                    request_id: Uuid::new_v4(),
                    expires_at: saved.expires_at,
                    auth: Some(auth),
                });
            }
            Err(e) if e.contains("HTTP 401.") => {
                store(&saved.origin, Store::Delete).await?;
            }
            Err(e) => return Err(e),
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| "Cannot listen for browser sign-in. Try again.")?;
    let redirect = format!(
        "http://{}/callback",
        listener
            .local_addr()
            .map_err(|_| "Cannot read sign-in address")?
    );
    let verifier = random_secret();
    let nonce = random_secret();
    let url = login_url(&origin, &redirect, &nonce, &verifier)?;
    let info = Progress {
        status: "pending".into(),
        request_id: Uuid::new_v4(),
        expires_at: Utc::now() + chrono::Duration::minutes(10),
        auth: None,
    };
    let (cancel, receiver) = watch::channel(());
    {
        let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
        if c.generation != generation {
            return Err("Connection changed".into());
        }
        c.pending = Some(Pending {
            generation,
            info: info.clone(),
            url,
            deadline: Instant::now() + Duration::from_secs(600),
            error: None,
            _cancel: cancel,
        });
    }
    tauri::async_runtime::spawn(async move {
        let result = tokio::time::timeout(
            Duration::from_secs(600),
            receive_callback(listener, &redirect, &nonce, receiver),
        )
        .await;
        let result = match result {
            Ok(Ok((code, mut socket))) => {
                let result = complete(&app, origin, generation, code, verifier, redirect).await;
                let message = if result.is_ok() {
                    "You are signed in. You can close this tab and return to Codifix."
                } else {
                    "Sign-in could not finish. Return to Codifix to try again."
                };
                respond(&mut socket, "200 OK", message).await;
                result
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err("Sign-in timed out. Try again.".into()),
        };
        if let Err(error) = result {
            if let Ok(mut c) = app.state::<Transport>().0.lock() {
                if c.generation == generation {
                    if let Some(p) = c.pending.as_mut() {
                        p.error = Some(error);
                    }
                }
            }
        }
    });
    Ok(info)
}
// Only a direct loopback navigation with the correct state can finish sign-in.
#[derive(Debug, PartialEq)]
enum Callback {
    Code(String),
    Denied,
}
fn callback_code(request: &str, redirect: &str, state: &str) -> Result<Callback, String> {
    let (headers, _) = request
        .split_once("\r\n\r\n")
        .ok_or("Incomplete callback")?;
    let mut lines = headers.split("\r\n");
    let parts: Vec<_> = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    if parts.len() != 3 || parts[0] != "GET" || !matches!(parts[2], "HTTP/1.1" | "HTTP/1.0") {
        return Err("Invalid callback method".into());
    }
    let base = Url::parse(redirect).map_err(|_| "Invalid callback address")?;
    let expected_host = format!("127.0.0.1:{}", base.port().ok_or("Missing callback port")?);
    let hosts: Vec<_> = lines
        .filter_map(|s| s.split_once(':'))
        .filter(|(k, _)| k.eq_ignore_ascii_case("host"))
        .map(|(_, v)| v.trim())
        .collect();
    if hosts != [expected_host.as_str()] || !parts[1].starts_with("/callback?") {
        return Err("Invalid callback host or path".into());
    }
    let url = base.join(parts[1]).map_err(|_| "Invalid callback")?;
    if url.path() != "/callback" || url.fragment().is_some() {
        return Err("Invalid callback path".into());
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    let states: Vec<_> = pairs.iter().filter(|(k, _)| k == "state").collect();
    if states.len() != 1 || states[0].1 != state {
        return Err("Invalid callback state".into());
    }
    if pairs.len() == 2
        && pairs
            .iter()
            .any(|(k, v)| k == "error" && v == "access_denied")
    {
        return Ok(Callback::Denied);
    }
    let codes: Vec<_> = pairs.iter().filter(|(k, _)| k == "code").collect();
    if pairs.len() != 2
        || codes.len() != 1
        || codes[0].1.len() < 32
        || codes[0].1.len() > 256
        || !codes[0]
            .1
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("Invalid callback code".into());
    }
    Ok(Callback::Code(codes[0].1.to_string()))
}
async fn receive_callback(
    listener: TcpListener,
    redirect: &str,
    state: &str,
    mut cancel: watch::Receiver<()>,
) -> Result<(String, TcpStream), String> {
    loop {
        tokio::select! {
            _ = cancel.changed() => return Err("Sign-in cancelled".into()),
            accepted = listener.accept() => {
                let (mut socket, peer) = accepted.map_err(|_| "Cannot receive browser sign-in")?;
                if !peer.ip().is_loopback() { continue; }
                let request = tokio::time::timeout(Duration::from_secs(3), async {
                    let mut bytes = Vec::new();
                    loop {
                        let mut chunk = [0; 1024];
                        let n = socket.read(&mut chunk).await.map_err(|_| ())?;
                        if n == 0 || bytes.len() + n > 8192 { return Err(()); }
                        bytes.extend_from_slice(&chunk[..n]);
                        if bytes.windows(4).any(|w| w == b"\r\n\r\n") { return String::from_utf8(bytes).map_err(|_| ()); }
                    }
                }).await;
                if let Ok(Ok(request)) = request {
                    match callback_code(&request, redirect, state) {
                        Ok(Callback::Code(code)) => return Ok((code, socket)),
                        Ok(Callback::Denied) => {
                            respond(&mut socket, "200 OK", "Sign-in cancelled. Return to Codifix to try again.").await;
                            return Err("Sign-in was cancelled in your browser. Try again.".into());
                        }
                        Err(_) => {}
                    }
                }
                respond(&mut socket, "400 Bad Request", "This sign-in link is invalid. Return to Codifix and try again.").await;
            }
        }
    }
}
async fn respond(socket: &mut TcpStream, status: &str, message: &str) {
    let body = format!("<!doctype html><html><head><meta charset='utf-8'><title>Codifix sign-in</title></head><body><h1>Codifix</h1><p>{message}</p></body></html>");
    let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n{body}", body.len());
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        socket.write_all(response.as_bytes()),
    )
    .await;
}
async fn revoke(
    origin: &Url,
    e: &Enrollment,
    binding: Option<&crate::area_routing::Binding>,
) -> Result<(), String> {
    let mut session = Session::new(origin.clone(), Some(e.credential.clone()))?;
    session.area = binding.cloned();
    let receipt = session
        .request("/v1/auth/desktop/logout", Method::POST, Some(json!({})))
        .await?;
    if receipt["logged_out"] != true {
        return Err("Sign-out was not confirmed".into());
    }
    Ok(())
}
async fn complete(
    app: &tauri::AppHandle,
    origin: Url,
    generation: u64,
    code: String,
    verifier: String,
    redirect: String,
) -> Result<(), String> {
    let browser = app.state::<BrowserAuth>();
    let state = app.state::<Transport>();
    let _operation = browser.0.lock().await;
    current(&state, generation)?;
    let binding = crate::area_routing::selected(&state, &origin)?;
    crate::area_routing::verify(&origin, binding.as_ref()).await?;
    current(&state, generation)?;
    let mut exchange_session = Session::new(origin.clone(), None)?;
    exchange_session.area = binding.clone();
    exchange_session.fence = Some((state.1.clone(), generation));
    let exchange = exchange_session
        .request(
            "/v1/auth/desktop/exchange",
            Method::POST,
            Some(json!({"code":code,"code_verifier":verifier,"redirect_uri":redirect})),
        )
        .await?;
    #[derive(Deserialize)]
    struct Receipt {
        token: String,
        principal_id: Uuid,
        expires_at: DateTime<Utc>,
    }
    let receipt: Receipt =
        serde_json::from_value(exchange).map_err(|_| "Invalid sign-in response")?;
    let mut saved = Enrollment {
        origin: origin.origin().ascii_serialization(),
        credential: receipt.token,
        principal_id: receipt.principal_id,
        expires_at: receipt.expires_at,
        pending_logout: false,
    };
    if !valid_record(&saved, &saved.origin) {
        return Err("Invalid sign-in response".into());
    }
    let activation = async {
        if saved.expires_at <= Utc::now() {
            return Err("Sign-in expired. Try again.".into());
        }
        let mut session = Session::new(origin.clone(), Some(saved.credential.clone()))?;
        session.area = binding.clone();
        session.fence = Some((state.1.clone(), generation));
        let auth = session.request("/v1/auth/me", Method::GET, None).await?;
        validate_auth(&auth, &saved)?;
        current(&state, generation)?;
        store(&saved.origin, Store::Write(saved.clone())).await?;
        let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
        if c.generation != generation {
            return Err("Sign-in cancelled".into());
        }
        c.session = Some(session);
        if let Some(p) = c.pending.as_mut() {
            p.info.status = "approved".into();
            p.info.auth = Some(auth);
        }
        Ok::<_, String>(())
    }
    .await;
    if let Err(error) = activation {
        // Retain a revocation marker if cleanup cannot be confirmed. Never activate it.
        saved.pending_logout = true;
        if revoke(&origin, &saved, binding.as_ref()).await.is_ok() {
            let _ = store(&saved.origin, Store::Delete).await;
            return Err(error);
        }
        browser
            .1
            .lock()
            .map_err(|_| "Sign-in state unavailable")?
            .push(saved.clone());
        if store(&saved.origin, Store::Write(saved.clone()))
            .await
            .is_err()
        {
            return Err(format!("{error} Could not revoke or save the session for cleanup. Keep Codifix open, unlock your keyring, restore the connection, and retry Sign out."));
        }
        return Err(format!("{error} Sign-out is unconfirmed; retry Sign out."));
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    Ok(())
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
                && p.info.request_id.to_string() == request_id
                && Instant::now() < p.deadline
                && p.error.is_none()
        })
        .ok_or("No current sign-in request")?;
    open::that(p.url.as_str()).map_err(|_| {
        "Could not open your default browser. Check your desktop browser settings and retry.".into()
    })
}
#[tauri::command]
pub(crate) fn poll_browser_login(
    request_id: String,
    state: State<'_, Transport>,
) -> Result<Progress, String> {
    let c = state.0.lock().map_err(|_| "Connection unavailable")?;
    let p = c
        .pending
        .as_ref()
        .filter(|p| p.generation == c.generation && p.info.request_id.to_string() == request_id)
        .ok_or("No current sign-in request")?;
    if let Some(error) = &p.error {
        return Err(error.clone());
    }
    Ok(p.info.clone())
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
        state
            .1
            .store(c.generation, std::sync::atomic::Ordering::SeqCst);
        c.session = None;
        c.pending = None;
    }
    let _operation = browser.0.lock().await;
    let binding = crate::area_routing::selected(&state, &origin)?;
    crate::area_routing::verify(&origin, binding.as_ref()).await?;
    let name = origin.origin().ascii_serialization();
    let outstanding: Vec<_> = browser
        .1
        .lock()
        .map_err(|_| "Sign-in state unavailable")?
        .iter()
        .filter(|e| e.origin == name)
        .cloned()
        .collect();
    for saved in outstanding {
        revoke(&origin, &saved, binding.as_ref())
            .await
            .map_err(|e| {
                format!("{e} Sign-out is unconfirmed; keep Codifix open and retry Sign out.")
            })?;
        browser
            .1
            .lock()
            .map_err(|_| "Sign-in state unavailable")?
            .retain(|e| e.credential != saved.credential);
    }
    if let Some(mut saved) = store(&name, Store::Read).await? {
        saved.pending_logout = true;
        browser
            .1
            .lock()
            .map_err(|_| "Sign-in state unavailable")?
            .push(saved.clone());
        let _ = store(&name, Store::Write(saved.clone())).await;
        revoke(&origin, &saved, binding.as_ref())
            .await
            .map_err(|e| format!("{e} Sign-out is unconfirmed; retry Sign out."))?;
        browser
            .1
            .lock()
            .map_err(|_| "Sign-in state unavailable")?
            .retain(|e| e.credential != saved.credential);
        store(&name, Store::Delete).await?;
    }
    Ok(())
}
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
        state
            .1
            .store(c.generation, std::sync::atomic::Ordering::SeqCst);
        c.session = None;
        c.pending = None;
    }
    let _operation = browser.0.lock().await;
    store(&origin, Store::Delete).await?;
    browser
        .1
        .lock()
        .map_err(|_| "Sign-in state unavailable")?
        .retain(|e| e.origin != origin);
    Ok(())
}
#[tauri::command]
pub(crate) async fn revoke_legacy_browser_login(
    endpoint: String,
    state: State<'_, Transport>,
    browser: State<'_, BrowserAuth>,
) -> Result<(), String> {
    let origin = crate::endpoint(&endpoint)?;
    let _operation = browser.0.lock().await;
    let binding = crate::area_routing::selected(&state, &origin)?;
    crate::area_routing::verify(&origin, binding.as_ref()).await?;
    crate::legacy_browser_auth::cleanup(&origin, binding).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "uses a disposable entry in the current desktop keyring"]
    async fn host_native_keyring_roundtrip() {
        let origin = format!("https://test-{}.invalid", Uuid::new_v4());
        let saved = Enrollment {
            origin: origin.clone(),
            credential: format!("mld_{}", random_secret()),
            principal_id: Uuid::new_v4(),
            expires_at: Utc::now() + chrono::Duration::minutes(1),
            pending_logout: false,
        };
        assert!(store(&origin, Store::Read).await.unwrap().is_none());
        store(&origin, Store::Write(saved.clone())).await.unwrap();
        let read = store(&origin, Store::Read).await.unwrap().unwrap();
        store(&origin, Store::Delete).await.unwrap();
        assert_eq!(read.credential, saved.credential);
        assert_eq!(read.principal_id, saved.principal_id);
        assert!(store(&origin, Store::Read).await.unwrap().is_none());
    }
    #[test]
    fn pkce_uses_s256_and_never_exports_verifier() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let url = login_url(
            &Url::parse("https://service.example").unwrap(),
            "http://127.0.0.1:12345/callback",
            "state",
            verifier,
        )
        .unwrap();
        assert_eq!(
            url.query_pairs()
                .find(|(k, _)| k == "code_challenge")
                .unwrap()
                .1,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert!(!url.as_str().contains(verifier));
    }
    #[test]
    fn callback_rejects_wrong_host_state_path_method_and_duplicate_parameters() {
        let redirect = "http://127.0.0.1:12345/callback";
        let state = random_secret();
        let code = random_secret();
        let request = format!(
            "GET /callback?code={code}&state={state} HTTP/1.1\r\nHost: 127.0.0.1:12345\r\n\r\n"
        );
        assert_eq!(
            callback_code(&request, redirect, &state).unwrap(),
            Callback::Code(code)
        );
        let denied = format!("GET /callback?error=access_denied&state={state} HTTP/1.1\r\nHost: 127.0.0.1:12345\r\n\r\n");
        assert_eq!(
            callback_code(&denied, redirect, &state).unwrap(),
            Callback::Denied
        );
        for invalid in [
            request.replace("GET", "POST"),
            request.replace("Host: 127.0.0.1", "Host: localhost"),
            request.replace("/callback?", "/elsewhere?"),
            request.replace(&state, "wrong"),
            request.replace(" HTTP/1.1", &format!("&state={state} HTTP/1.1")),
            request.replace("Host:", "Host: evil.example\r\nHost:"),
        ] {
            assert!(callback_code(&invalid, redirect, &state).is_err());
        }
    }
    #[test]
    fn identity_origin_and_generation_are_bound() {
        let saved = Enrollment {
            origin: "https://service.example".into(),
            credential: format!("mld_{}", random_secret()),
            principal_id: Uuid::new_v4(),
            expires_at: Utc::now(),
            pending_logout: false,
        };
        assert!(valid_record(&saved, &saved.origin));
        assert!(!valid_record(&saved, "https://other.example"));
        let mut auth = json!({"authenticated":true,"mode":"multi_user","principal":{"id":saved.principal_id,"kind":"human_oidc"}});
        validate_auth(&auth, &saved).unwrap();
        auth["principal"]["kind"] = json!("service_token");
        assert!(validate_auth(&auth, &saved).is_err());
        let state = Transport::default();
        current(&state, 0).unwrap();
        state.0.lock().unwrap().generation += 1;
        assert!(current(&state, 0).is_err());
        let progress = Progress {
            status: "pending".into(),
            request_id: Uuid::new_v4(),
            expires_at: Utc::now(),
            auth: None,
        };
        assert!(!serde_json::to_string(&progress)
            .unwrap()
            .contains(&saved.credential));
    }
    #[tokio::test]
    async fn real_listener_ignores_bad_callbacks_then_accepts_matching_callback() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let redirect = format!("http://{addr}/callback");
        let state = random_secret();
        let code = random_secret();
        let (cancel, rx) = watch::channel(());
        let sender_state = state.clone();
        let sender_code = code.clone();
        let sender = tokio::spawn(async move {
            for nonce in ["wrong".to_owned(), sender_state] {
                let mut socket = TcpStream::connect(addr).await.unwrap();
                socket.write_all(format!("GET /callback?code={sender_code}&state={nonce} HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes()).await.unwrap();
                let mut response = String::new();
                socket.read_to_string(&mut response).await.unwrap();
                assert!(response.starts_with(if nonce == "wrong" {
                    "HTTP/1.1 400"
                } else {
                    "HTTP/1.1 200"
                }));
            }
        });
        let (received, mut socket) = receive_callback(listener, &redirect, &state, rx)
            .await
            .unwrap();
        assert_eq!(received, code);
        respond(&mut socket, "200 OK", "Signed in").await;
        drop(socket);
        sender.await.unwrap();
        drop(cancel);
    }
    #[test]
    fn unpersisted_revocation_blocks_relogin_for_only_its_origin() {
        let origin = Url::parse("https://service.example").unwrap();
        let browser = BrowserAuth::default();
        browser.check_cleanup(&origin).unwrap();
        browser.1.lock().unwrap().push(Enrollment {
            origin: origin.origin().ascii_serialization(),
            credential: format!("mld_{}", random_secret()),
            principal_id: Uuid::new_v4(),
            expires_at: Utc::now(),
            pending_logout: true,
        });
        assert!(browser.check_cleanup(&origin).is_err());
        browser
            .check_cleanup(&Url::parse("https://other.example").unwrap())
            .unwrap();
    }
    #[tokio::test]
    async fn browser_denial_finishes_instead_of_waiting_until_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = random_secret();
        let (tx, rx) = watch::channel(());
        let sender_state = state.clone();
        let sender = tokio::spawn(async move {
            let mut socket = TcpStream::connect(addr).await.unwrap();
            socket.write_all(format!("GET /callback?error=access_denied&state={sender_state} HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes()).await.unwrap();
            let mut body = String::new();
            socket.read_to_string(&mut body).await.unwrap();
            assert!(body.contains("Sign-in cancelled"));
        });
        let result =
            receive_callback(listener, &format!("http://{addr}/callback"), &state, rx).await;
        assert!(matches!(result, Err(e) if e.contains("cancelled in your browser")));
        sender.await.unwrap();
        drop(tx);
    }
    #[tokio::test]
    async fn cancellation_closes_ephemeral_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = watch::channel(());
        drop(tx);
        assert!(
            receive_callback(listener, &format!("http://{addr}/callback"), "state", rx)
                .await
                .is_err()
        );
        assert!(TcpStream::connect(addr).await.is_err());
    }
}
