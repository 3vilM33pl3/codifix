// SPDX-License-Identifier: AGPL-3.0-or-later
//! Human-only terminal Inbox transport. Credentials are memory-only and never
//! share the CLI service-token client, keyring, cookies, or diagnostics.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use reqwest::{Client, Method, Url};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
};
use uuid::Uuid;

#[derive(Clone)]
pub(crate) struct InboxTarget {
    origin: Url,
    area_instance_id: Uuid,
}
impl InboxTarget {
    pub(crate) fn new(origin: &str, area_instance_id: Uuid) -> Result<Self, String> {
        let origin = Url::parse(origin).map_err(|_| "Invalid Inbox browser origin")?;
        if origin.scheme() != "https"
            || origin.host_str().is_none()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
            || area_instance_id.is_nil()
        {
            return Err(
                "Inbox requires an HTTPS browser origin and a selected area identity".into(),
            );
        }
        Ok(Self {
            origin,
            area_instance_id,
        })
    }
    /// Bind only public metadata from the protected selected config. Environment
    /// overlays cannot silently redirect a human browser or alter its area.
    pub(crate) fn from_config(config: &codifix_config::AppConfig) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct PublicConfig {
            area: Option<codifix_config::AreaIdentityConfig>,
            auth: Option<PublicAuth>,
        }
        #[derive(Deserialize)]
        struct PublicAuth {
            public_base_url: Option<String>,
        }
        let path = config
            .resolved_config_path
            .as_deref()
            .ok_or("Inbox requires a selected area config file")?;
        const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
        #[cfg(unix)]
        let text = {
            use std::io::Read;
            use std::os::unix::fs::{MetadataExt, PermissionsExt};

            let descriptor = rustix::fs::open(
                path,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC
                    | rustix::fs::OFlags::NONBLOCK,
                rustix::fs::Mode::empty(),
            )
            .map_err(|_| "Selected area config unavailable")?;
            let file = std::fs::File::from(descriptor);
            let metadata = file
                .metadata()
                .map_err(|_| "Selected area config unavailable")?;
            if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
                return Err("Selected area config must be a bounded regular file".into());
            }
            if metadata.permissions().mode() & 0o022 != 0
                || (metadata.uid() != 0 && metadata.uid() != rustix::process::geteuid().as_raw())
            {
                return Err("Inbox config must belong to this user or root and must not be group/world writable".into());
            }
            let mut text = String::new();
            let read = file
                .take(MAX_CONFIG_BYTES + 1)
                .read_to_string(&mut text)
                .map_err(|_| "Selected area config unavailable")?;
            if read as u64 > MAX_CONFIG_BYTES {
                return Err("Selected area config must be a bounded regular file".into());
            }
            text
        };
        #[cfg(windows)]
        let text = String::from_utf8(
            codifix_platform::private_windows::Input::open(path, MAX_CONFIG_BYTES)
                .and_then(|input| input.read(MAX_CONFIG_BYTES))
                .map_err(|_| "Inbox config requires protected Windows ACLs")?,
        )
        .map_err(|_| "Invalid Inbox public config")?;
        let public: PublicConfig =
            toml::from_str(&text).map_err(|_| "Invalid Inbox public config")?;
        if public.area.is_none() || public.area != config.area {
            return Err(
                "Selected area config identity changed or differs from effective config".into(),
            );
        }
        let origin = public
            .auth
            .and_then(|a| a.public_base_url)
            .ok_or("Selected area config requires auth.public_base_url for Inbox")?;
        if config.auth.public_base_url.as_deref() != Some(origin.as_str()) {
            return Err("Effective Inbox origin differs from selected area config".into());
        }
        Self::new(
            &origin,
            public
                .area
                .ok_or("Inbox requires a selected area")?
                .instance_id,
        )
    }
}
#[derive(Clone, Debug)]
pub(crate) struct BrowserLogin {
    pub request_id: Uuid,
    pub expires_at: DateTime<Utc>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoginStatus {
    Pending,
    Approved,
}
#[derive(Clone, Debug)]
pub(crate) struct LoginProgress {
    pub status: LoginStatus,
    pub auth: Option<Value>,
}
// Deliberately no Debug or Serialize for credentials or client state.
#[derive(Clone, Deserialize)]
struct Credential {
    token: String,
    principal_id: Uuid,
    expires_at: DateTime<Utc>,
}
struct Pending {
    info: BrowserLogin,
    url: Url,
    deadline: Instant,
    progress: Result<LoginProgress, String>,
    _cancel: watch::Sender<()>,
}
#[derive(Default)]
struct State {
    generation: u64,
    session: Option<Credential>,
    cleanup: Vec<Credential>,
    pending: Option<Pending>,
}
impl State {
    fn expire_credentials(&mut self) {
        let now = Utc::now();
        if self.session.as_ref().is_some_and(|c| c.expires_at <= now) {
            self.session = None;
            self.generation = self.generation.wrapping_add(1);
        }
        self.cleanup.retain(|c| c.expires_at > now);
    }
}
#[derive(Clone)]
pub(crate) struct HumanInbox {
    target: InboxTarget,
    client: Client,
    state: Arc<Mutex<State>>,
    operation: Arc<tokio::sync::Mutex<()>>,
}
impl HumanInbox {
    pub(crate) fn new(target: InboxTarget) -> Result<Self, String> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| "Cannot create Inbox client")?;
        Ok(Self {
            target,
            client,
            state: Arc::new(Mutex::new(State::default())),
            operation: Arc::new(tokio::sync::Mutex::new(())),
        })
    }
    pub(crate) fn logged_in(&self) -> bool {
        self.state.lock().is_ok_and(|mut s| {
            s.expire_credentials();
            s.session.is_some()
        })
    }
    /// Immediately fence all old work; retain credentials solely for revocation.
    pub(crate) fn cancel_login(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.generation = s.generation.wrapping_add(1);
            s.pending = None;
            if let Some(c) = s.session.take() {
                s.cleanup.push(c);
            }
        }
    }
    fn current(&self, generation: u64) -> Result<(), String> {
        if self
            .state
            .lock()
            .map_err(|_| "Inbox state unavailable")?
            .generation
            != generation
        {
            return Err("Inbox connection changed; discard this result".into());
        }
        Ok(())
    }
    async fn raw(
        &self,
        path: &str,
        method: Method,
        credential: Option<&Credential>,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let url = self
            .target
            .origin
            .join(path)
            .map_err(|_| "Invalid Inbox API path")?;
        let mut request = self
            .client
            .request(method, url)
            .header("origin", self.target.origin.origin().ascii_serialization())
            .header("cache-control", "no-store");
        if let Some(c) = credential {
            request = request.bearer_auth(&c.token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| "Inbox connection failed; check the address, TLS trust and network")?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("Inbox returned HTTP {}", status.as_u16()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Inbox response interrupted")?
        {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err("Inbox response exceeds size limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| "Inbox returned invalid JSON".into())
    }
    async fn verify(&self) -> Result<(), String> {
        let health = self.raw("/healthz", Method::GET, None, None).await?;
        if health["area_instance_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            != Some(self.target.area_instance_id)
        {
            return Err("Inbox area identity mismatch; no credentials were sent".into());
        }
        Ok(())
    }
    pub(crate) async fn begin_login(&self) -> Result<BrowserLogin, String> {
        let _operation = self.operation.lock().await;
        let generation = {
            let mut s = self.state.lock().map_err(|_| "Inbox state unavailable")?;
            s.expire_credentials();
            if s.session.is_some() || !s.cleanup.is_empty() {
                return Err("Sign out before starting another Inbox login".into());
            }
            s.generation = s.generation.wrapping_add(1);
            s.pending = None;
            s.generation
        };
        self.verify().await?;
        self.current(generation)?;
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "Cannot listen for browser sign-in")?;
        let redirect = format!(
            "http://{}/callback",
            listener
                .local_addr()
                .map_err(|_| "Cannot read callback address")?
        );
        let verifier = random_secret();
        let nonce = random_secret();
        let mut url = self
            .target
            .origin
            .join("/v1/auth/desktop/login")
            .map_err(|_| "Invalid sign-in address")?;
        url.query_pairs_mut()
            .append_pair("redirect_uri", &redirect)
            .append_pair("state", &nonce)
            .append_pair(
                "code_challenge",
                &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
            )
            .append_pair("code_challenge_method", "S256");
        let info = BrowserLogin {
            request_id: Uuid::new_v4(),
            expires_at: Utc::now() + chrono::Duration::minutes(10),
        };
        let (cancel, receiver) = watch::channel(());
        {
            let mut s = self.state.lock().map_err(|_| "Inbox state unavailable")?;
            if s.generation != generation {
                return Err("Inbox sign-in cancelled".into());
            }
            s.pending = Some(Pending {
                info: info.clone(),
                url,
                deadline: Instant::now() + Duration::from_secs(600),
                progress: Ok(LoginProgress {
                    status: LoginStatus::Pending,
                    auth: None,
                }),
                _cancel: cancel,
            });
        }
        let weak_state = Arc::downgrade(&self.state);
        let target = self.target.clone();
        let client = self.client.clone();
        let operation = self.operation.clone();
        tokio::spawn(async move {
            let received = tokio::time::timeout(
                Duration::from_secs(600),
                receive_callback(listener, &redirect, &nonce, receiver),
            )
            .await;
            let Some(state) = weak_state.upgrade() else {
                return;
            };
            let this = Self {
                target,
                client,
                state,
                operation,
            };
            let result = match received {
                Ok(Ok((code, mut socket))) => {
                    let result = this.complete(generation, code, verifier, redirect).await;
                    respond(
                        &mut socket,
                        "200 OK",
                        if result.is_ok() {
                            "You are signed in. Return to Codifix."
                        } else {
                            "Sign-in could not finish. Return to Codifix."
                        },
                    )
                    .await;
                    result
                }
                Ok(Err(error)) => Err(error),
                Err(_) => Err("Inbox sign-in timed out; try again".into()),
            };
            if let Ok(mut s) = this.state.lock() {
                if s.generation == generation {
                    if let Some(p) = s.pending.as_mut() {
                        p.progress = result;
                    }
                }
            }
        });
        Ok(info)
    }
    pub(crate) async fn open_login(&self, request_id: Uuid) -> Result<(), String> {
        let (url, generation) = {
            let s = self.state.lock().map_err(|_| "Inbox state unavailable")?;
            let p = s
                .pending
                .as_ref()
                .filter(|p| {
                    p.info.request_id == request_id
                        && Instant::now() < p.deadline
                        && p.progress
                            .as_ref()
                            .is_ok_and(|p| p.status == LoginStatus::Pending)
                })
                .ok_or("No current Inbox sign-in request")?;
            (p.url.clone(), s.generation)
        };
        self.verify().await?;
        // Keep the generation locked through browser launch, so a concurrent
        // cancellation cannot open an obsolete authorization request.
        let state = self.state.lock().map_err(|_| "Inbox state unavailable")?;
        if state.generation != generation {
            return Err("Inbox sign-in cancelled".into());
        }
        open_browser(url.as_str())
    }
    pub(crate) fn poll_login(&self, request_id: Uuid) -> Result<LoginProgress, String> {
        let s = self.state.lock().map_err(|_| "Inbox state unavailable")?;
        let p = s
            .pending
            .as_ref()
            .filter(|p| p.info.request_id == request_id)
            .ok_or("No current Inbox sign-in request")?;
        if Instant::now() >= p.deadline {
            return Err("Inbox sign-in timed out; try again".into());
        }
        p.progress.clone()
    }
    async fn complete(
        &self,
        generation: u64,
        code: String,
        verifier: String,
        redirect: String,
    ) -> Result<LoginProgress, String> {
        let _operation = self.operation.lock().await;
        self.verify().await?;
        self.current(generation)?;
        let receipt = self
            .raw(
                "/v1/auth/desktop/exchange",
                Method::POST,
                None,
                Some(json!({"code": code, "code_verifier": verifier, "redirect_uri": redirect})),
            )
            .await?;
        let credential: Credential =
            serde_json::from_value(receipt).map_err(|_| "Invalid Inbox sign-in response")?;
        if !valid_credential(&credential) {
            return Err("Invalid Inbox sign-in credential".into());
        }
        // Capture the receipt before any cancellable checks. A stale exchange
        // can only enter revocation cleanup, never the active session.
        self.state
            .lock()
            .map_err(|_| "Inbox state unavailable")?
            .cleanup
            .push(credential.clone());
        let activation = async {
            self.current(generation)?;
            self.verify().await?;
            self.current(generation)?;
            let auth = self
                .raw("/v1/auth/me", Method::GET, Some(&credential), None)
                .await?;
            validate_auth(&auth, &credential)?;
            self.verify().await?;
            let mut s = self.state.lock().map_err(|_| "Inbox state unavailable")?;
            if s.generation != generation {
                return Err("Inbox sign-in cancelled".into());
            }
            s.cleanup.retain(|c| c.token != credential.token);
            s.session = Some(credential.clone());
            Ok(LoginProgress {
                status: LoginStatus::Approved,
                auth: Some(auth),
            })
        }
        .await;
        if activation.is_err() {
            if self.revoke(&credential).await.is_ok() {
                self.state
                    .lock()
                    .map_err(|_| "Inbox state unavailable")?
                    .cleanup
                    .retain(|c| c.token != credential.token);
            } else {
                return Err("Inbox sign-in failed and sign-out is unconfirmed; keep Codifix open and retry Sign out".into());
            }
        }
        activation
    }
    async fn revoke(&self, credential: &Credential) -> Result<(), String> {
        self.verify().await?;
        let receipt = self
            .raw(
                "/v1/auth/desktop/logout",
                Method::POST,
                Some(credential),
                Some(json!({})),
            )
            .await?;
        if receipt["logged_out"] != true {
            return Err("Inbox sign-out was not confirmed".into());
        }
        Ok(())
    }
    pub(crate) async fn sign_out(&self) -> Result<(), String> {
        self.cancel_login();
        let _operation = self.operation.lock().await;
        let credentials = self
            .state
            .lock()
            .map_err(|_| "Inbox state unavailable")?
            .cleanup
            .clone();
        for c in credentials {
            self.revoke(&c).await.map_err(
                |_| "Inbox sign-out is unconfirmed; keep Codifix open and retry Sign out",
            )?;
            self.state
                .lock()
                .map_err(|_| "Inbox state unavailable")?
                .cleanup
                .retain(|entry| entry.token != c.token);
        }
        Ok(())
    }
    async fn read(&self, path: &str) -> Result<Value, String> {
        let (generation, credential) = {
            let s = self.state.lock().map_err(|_| "Inbox state unavailable")?;
            (
                s.generation,
                s.session
                    .clone()
                    .filter(|c| c.expires_at > Utc::now())
                    .ok_or("Sign in to My Inbox first")?,
            )
        };
        if let Err(error) = self.verify().await {
            if self.current(generation).is_ok() {
                self.cancel_login();
            }
            return Err(error);
        }
        self.current(generation)?;
        let result = self.raw(path, Method::GET, Some(&credential), None).await;
        if result
            .as_ref()
            .is_err_and(|e| e == "Inbox returned HTTP 401")
        {
            if let Ok(mut s) = self.state.lock() {
                if s.generation == generation {
                    if let Some(credential) = s.session.take() {
                        s.cleanup.push(credential);
                    }
                    s.pending = None;
                    s.generation = s.generation.wrapping_add(1);
                }
            }
        }
        self.current(generation)?;
        if let Err(error) = self.verify().await {
            if self.current(generation).is_ok() {
                self.cancel_login();
            }
            return Err(error);
        }
        self.current(generation)?;
        result
    }
    pub(crate) async fn list(&self) -> Result<Value, String> {
        self.read("/v1/inbox/conversations").await
    }
    pub(crate) async fn detail(&self, id: Uuid) -> Result<Value, String> {
        self.read(&format!("/v1/inbox/conversations/{id}")).await
    }
    pub(crate) async fn history(&self, id: Uuid, before: i64, limit: u16) -> Result<Value, String> {
        if before < 1 || !(1..=100).contains(&limit) {
            return Err("Invalid Inbox history cursor or limit".into());
        }
        self.read(&format!(
            "/v1/inbox/conversations/{id}/messages?before_sequence={before}&limit={limit}"
        ))
        .await
    }
    pub(crate) async fn network_status(&self) -> Result<Value, String> {
        self.read("/v1/network/status").await
    }
}
fn random_secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
fn valid_credential(c: &Credential) -> bool {
    !c.principal_id.is_nil()
        && c.expires_at > Utc::now()
        && (36..=256).contains(&c.token.len())
        && c.token.starts_with("mld_")
        && c.token[4..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn validate_auth(auth: &Value, c: &Credential) -> Result<(), String> {
    if auth["authenticated"] != true
        || auth["mode"] != "multi_user"
        || auth["principal"]["kind"] != "human_oidc"
        || auth["principal"]["id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            != Some(c.principal_id)
    {
        return Err("Inbox human sign-in identity could not be verified".into());
    }
    Ok(())
}
fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(windows)]
    let mut command = {
        let mut c = std::process::Command::new("rundll32.exe");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    return Err("Browser launch is unavailable on this platform".into());
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Cannot open your default browser; check browser settings and retry".into())
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
    let body = format!(
        "<!doctype html><html><head><meta charset='utf-8'><title>Codifix sign-in</title></head><body><h1>Codifix</h1><p>{message}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        socket.write_all(response.as_bytes()),
    )
    .await;
}

#[cfg(test)]
mod tests;
