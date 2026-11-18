// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

#[derive(Clone)]
struct Request {
    path: String,
    auth: bool,
    body: Value,
}
struct Mock {
    area: Mutex<Uuid>,
    principal: Uuid,
    human: AtomicBool,
    unauthorized: AtomicBool,
    logout_fails: AtomicBool,
    redirect_reads: AtomicBool,
    exchange_wait: AtomicBool,
    read_wait: AtomicBool,
    exchange_arrived: Notify,
    exchange_release: Notify,
    read_arrived: Notify,
    read_release: Notify,
    requests: Mutex<Vec<Request>>,
    challenge: Mutex<String>,
}
fn credential(principal: Uuid) -> Credential {
    Credential {
        token: format!("mld_{}", "x".repeat(48)),
        principal_id: principal,
        expires_at: Utc::now() + chrono::Duration::hours(1),
    }
}
async fn fixture() -> (HumanInbox, Arc<Mock>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let area = Uuid::new_v4();
    let mock = Arc::new(Mock {
        area: Mutex::new(area),
        principal: Uuid::new_v4(),
        human: AtomicBool::new(true),
        unauthorized: AtomicBool::new(false),
        logout_fails: AtomicBool::new(false),
        redirect_reads: AtomicBool::new(false),
        exchange_wait: AtomicBool::new(false),
        read_wait: AtomicBool::new(false),
        exchange_arrived: Notify::new(),
        exchange_release: Notify::new(),
        read_arrived: Notify::new(),
        read_release: Notify::new(),
        requests: Mutex::new(vec![]),
        challenge: Mutex::new(String::new()),
    });
    // Only this private test fixture bypasses the production HTTPS constructor.
    let target = InboxTarget {
        origin: Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap(),
        area_instance_id: area,
    };
    let inbox = HumanInbox::new(target).unwrap();
    let server = mock.clone();
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let m = server.clone();
            tokio::spawn(async move {
                let mut bytes = vec![];
                let (head_len, length) = loop {
                    let mut chunk = [0u8; 4096];
                    let n = socket.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(n) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..n]);
                        let length = headers
                            .lines()
                            .filter_map(|s| s.split_once(':'))
                            .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                            .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                            .unwrap_or(0);
                        break (n + 4, length);
                    }
                };
                while bytes.len() < head_len + length {
                    let mut chunk = [0u8; 4096];
                    let n = socket.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                }
                let headers = String::from_utf8_lossy(&bytes[..head_len]);
                let path = headers.split_whitespace().nth(1).unwrap().to_owned();
                let auth = headers.lines().any(|s| {
                    s.to_ascii_lowercase()
                        .starts_with("authorization: bearer mld_")
                });
                let body = serde_json::from_slice(&bytes[head_len..head_len + length])
                    .unwrap_or(Value::Null);
                m.requests.lock().unwrap().push(Request {
                    path: path.clone(),
                    auth,
                    body: body.clone(),
                });
                let (status, value) = if path == "/healthz" {
                    (200, json!({"area_instance_id":*m.area.lock().unwrap()}))
                } else if path == "/v1/auth/desktop/exchange" {
                    m.exchange_arrived.notify_one();
                    if m.exchange_wait.load(Ordering::SeqCst) {
                        m.exchange_release.notified().await;
                    }
                    let verifier = body["code_verifier"].as_str().unwrap();
                    assert_eq!(
                        *m.challenge.lock().unwrap(),
                        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
                    );
                    assert!(
                        body["redirect_uri"]
                            .as_str()
                            .unwrap()
                            .starts_with("http://127.0.0.1:")
                    );
                    assert!(!auth);
                    let c = credential(m.principal);
                    (
                        200,
                        json!({"token":c.token,"principal_id":c.principal_id,"expires_at":c.expires_at}),
                    )
                } else if path == "/v1/auth/me" {
                    assert!(auth);
                    (
                        200,
                        json!({"authenticated":true,"mode":"multi_user","principal":{"kind":if m.human.load(Ordering::SeqCst){"human_oidc"}else{"service"},"id":m.principal}}),
                    )
                } else if path == "/v1/auth/desktop/logout" {
                    assert!(auth);
                    if m.logout_fails.load(Ordering::SeqCst) {
                        (503, json!({"secret":"must not be displayed"}))
                    } else {
                        (200, json!({"logged_out":true}))
                    }
                } else {
                    assert!(auth);
                    m.read_arrived.notify_one();
                    if m.read_wait.load(Ordering::SeqCst) {
                        m.read_release.notified().await;
                    }
                    if m.redirect_reads.load(Ordering::SeqCst) {
                        (302, json!({"secret":"must not be displayed"}))
                    } else if m.unauthorized.load(Ordering::SeqCst) {
                        (401, json!({"secret":"must not be displayed"}))
                    } else if path == "/v1/network/status" {
                        (200, json!({"enabled":true,"identity":null}))
                    } else {
                        (200, json!([{"title":"private conversation"}]))
                    }
                };
                let body = serde_json::to_string(&value).unwrap();
                socket.write_all(format!("HTTP/1.1 {status} OK\r\nLocation: http://127.0.0.1:1/never-follow\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            });
        }
    });
    (inbox, mock, task)
}
async fn callback(
    inbox: &HumanInbox,
    m: &Mock,
    request_id: Uuid,
) -> tokio::task::JoinHandle<String> {
    let url = inbox
        .state
        .lock()
        .unwrap()
        .pending
        .as_ref()
        .unwrap()
        .url
        .clone();
    let pairs: std::collections::HashMap<_, _> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    *m.challenge.lock().unwrap() = pairs["code_challenge"].clone();
    let redirect = Url::parse(&pairs["redirect_uri"]).unwrap();
    let address = redirect.socket_addrs(|| None).unwrap()[0];
    let state = pairs["state"].clone();
    assert_eq!(
        inbox.poll_login(request_id).unwrap().status,
        LoginStatus::Pending
    );
    tokio::spawn(async move {
        let mut socket = TcpStream::connect(address).await.unwrap();
        socket
            .write_all(
                format!(
                    "GET /callback?state={state}&code={} HTTP/1.1\r\nHost: {address}\r\n\r\n",
                    "c".repeat(40)
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut result = vec![];
        socket.read_to_end(&mut result).await.unwrap();
        String::from_utf8(result).unwrap()
    })
}
async fn login(inbox: &HumanInbox, m: &Mock) -> Uuid {
    let info = inbox.begin_login().await.unwrap();
    let response = callback(inbox, m, info.request_id).await.await.unwrap();
    assert!(response.contains("You are signed in"));
    assert!(inbox.logged_in());
    info.request_id
}
#[test]
fn targets_require_https_origin_and_non_nil_area() {
    for origin in [
        "http://127.0.0.1:8080/",
        "https://user:pass@service.test/",
        "https://service.test/path",
        "https://service.test/?x=1",
        "https://service.test/#x",
    ] {
        assert!(InboxTarget::new(origin, Uuid::new_v4()).is_err());
    }
    assert!(InboxTarget::new("https://service.test/", Uuid::nil()).is_err());
    assert!(InboxTarget::new("https://service.test/", Uuid::new_v4()).is_ok());
}
#[tokio::test]
async fn native_pkce_login_private_reads_and_confirmed_logout() {
    let (inbox, m, server) = fixture().await;
    let id = login(&inbox, &m).await;
    assert_eq!(inbox.poll_login(id).unwrap().status, LoginStatus::Approved);
    assert_eq!(
        inbox.list().await.unwrap()[0]["title"],
        "private conversation"
    );
    assert_eq!(
        inbox.network_status().await.unwrap()["identity"],
        Value::Null
    );
    inbox.detail(Uuid::new_v4()).await.unwrap();
    inbox.history(Uuid::new_v4(), 101, 100).await.unwrap();
    assert!(inbox.history(Uuid::new_v4(), 0, 100).await.is_err());
    assert!(inbox.history(Uuid::new_v4(), 1, 101).await.is_err());
    inbox.sign_out().await.unwrap();
    assert!(!inbox.logged_in());
    assert!(inbox.list().await.is_err());
    let requests = m.requests.lock().unwrap();
    assert!(
        requests
            .iter()
            .filter(|r| r.path == "/healthz")
            .all(|r| !r.auth)
    );
    assert!(
        requests
            .iter()
            .any(|r| r.path.ends_with("before_sequence=101&limit=100"))
    );
    assert!(
        requests
            .iter()
            .any(|r| r.path == "/v1/auth/desktop/logout" && r.body == json!({}))
    );
    server.abort();
}
#[tokio::test]
async fn wrong_human_identity_never_activates_and_is_revoked() {
    let (inbox, m, server) = fixture().await;
    m.human.store(false, Ordering::SeqCst);
    let info = inbox.begin_login().await.unwrap();
    let response = callback(&inbox, &m, info.request_id).await.await.unwrap();
    assert!(response.contains("could not finish"));
    assert!(!inbox.logged_in());
    assert!(inbox.poll_login(info.request_id).is_err());
    assert!(
        m.requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.path == "/v1/auth/desktop/logout")
    );
    server.abort();
}
#[tokio::test]
async fn cancellation_during_exchange_cleans_remote_session() {
    let (inbox, m, server) = fixture().await;
    m.exchange_wait.store(true, Ordering::SeqCst);
    let info = inbox.begin_login().await.unwrap();
    let response = callback(&inbox, &m, info.request_id).await;
    m.exchange_arrived.notified().await;
    inbox.cancel_login();
    m.exchange_release.notify_one();
    response.await.unwrap();
    assert!(!inbox.logged_in());
    assert!(inbox.poll_login(info.request_id).is_err());
    assert!(inbox.state.lock().unwrap().cleanup.is_empty());
    assert!(
        m.requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.path == "/v1/auth/desktop/logout")
    );
    server.abort();
}
#[tokio::test]
async fn area_mismatch_fences_session_before_sending_credentials() {
    let (inbox, m, server) = fixture().await;
    login(&inbox, &m).await;
    m.requests.lock().unwrap().clear();
    *m.area.lock().unwrap() = Uuid::new_v4();
    assert!(
        inbox
            .list()
            .await
            .unwrap_err()
            .contains("area identity mismatch")
    );
    assert!(!inbox.logged_in());
    assert!(m.requests.lock().unwrap().iter().all(|r| !r.auth));
    server.abort();
}
#[tokio::test]
async fn cancelled_read_cannot_return_private_data() {
    let (inbox, m, server) = fixture().await;
    login(&inbox, &m).await;
    m.read_wait.store(true, Ordering::SeqCst);
    let clone = inbox.clone();
    let read = tokio::spawn(async move { clone.list().await });
    m.read_arrived.notified().await;
    inbox.cancel_login();
    m.read_release.notify_one();
    assert!(read.await.unwrap().is_err());
    assert!(!inbox.logged_in());
    server.abort();
}
#[tokio::test]
async fn unauthorized_clears_session_and_hides_error_body() {
    let (inbox, m, server) = fixture().await;
    login(&inbox, &m).await;
    m.unauthorized.store(true, Ordering::SeqCst);
    let error = inbox.list().await.unwrap_err();
    assert!(!error.contains("secret"));
    assert!(!inbox.logged_in());
    server.abort();
}
#[tokio::test]
async fn failed_logout_retains_revoke_only_marker_and_blocks_relogin() {
    let (inbox, m, server) = fixture().await;
    login(&inbox, &m).await;
    m.logout_fails.store(true, Ordering::SeqCst);
    let error = inbox.sign_out().await.unwrap_err();
    assert!(error.contains("unconfirmed"));
    assert!(!error.contains("secret"));
    assert!(!inbox.logged_in());
    assert!(inbox.begin_login().await.is_err());
    assert_eq!(inbox.state.lock().unwrap().cleanup.len(), 1);
    m.logout_fails.store(false, Ordering::SeqCst);
    inbox.sign_out().await.unwrap();
    assert!(inbox.state.lock().unwrap().cleanup.is_empty());
    server.abort();
}
#[tokio::test]
async fn malformed_callbacks_do_not_consume_listener() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let redirect = format!("http://{address}/callback");
    let (_cancel, receiver) = watch::channel(());
    let received = tokio::spawn(async move {
        receive_callback(listener, &redirect, "valid-state", receiver).await
    });
    for query in [
        "state=wrong&code=cccccccccccccccccccccccccccccccccccccccc",
        "state=valid-state&state=valid-state&code=cccccccccccccccccccccccccccccccccccccccc",
    ] {
        let mut socket = TcpStream::connect(address).await.unwrap();
        socket
            .write_all(
                format!("GET /callback?{query} HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes(),
            )
            .await
            .unwrap();
        let mut response = vec![];
        socket.read_to_end(&mut response).await.unwrap();
        assert!(
            String::from_utf8(response)
                .unwrap()
                .contains("400 Bad Request")
        );
    }
    let mut socket = TcpStream::connect(address).await.unwrap();
    socket
        .write_all(
            format!(
                "GET /callback?state=valid-state&code={} HTTP/1.1\r\nHost: {address}\r\n\r\n",
                "c".repeat(40)
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let (code, _socket) = received.await.unwrap().unwrap();
    assert_eq!(code, "c".repeat(40));
}
#[tokio::test]
async fn dropping_final_client_closes_callback_listener() {
    let (inbox, m, server) = fixture().await;
    inbox.begin_login().await.unwrap();
    let weak = Arc::downgrade(&inbox.state);
    drop(inbox);
    assert!(weak.upgrade().is_none());
    drop(m);
    server.abort();
}

#[test]
fn callback_protocol_requires_exact_host_method_path_and_parameters() {
    let redirect = "http://127.0.0.1:4242/callback";
    let good = format!(
        "GET /callback?state=state&code={} HTTP/1.1\r\nHost: 127.0.0.1:4242\r\n\r\n",
        "c".repeat(40)
    );
    assert!(matches!(
        callback_code(&good, redirect, "state"),
        Ok(Callback::Code(_))
    ));
    for bad in [
        good.replace("GET ", "POST "),
        good.replace("Host: 127.0.0.1:4242", "Host: attacker.test"),
        good.replace("/callback?", "/other?"),
        good.replace("state=state", "state=wrong"),
        good.replace("Host:", "Host: 127.0.0.1:4242\r\nHost:"),
        good.replace("&code=", "&extra=x&code="),
    ] {
        assert!(callback_code(&bad, redirect, "state").is_err());
    }
    assert_eq!(callback_code("GET /callback?state=state&error=access_denied HTTP/1.1\r\nHost: 127.0.0.1:4242\r\n\r\n",redirect,"state").unwrap(),Callback::Denied);
}
#[test]
fn trusted_public_metadata_must_match_effective_config() {
    let path = std::env::temp_dir().join(format!("codifix-inbox-config-{}.toml", Uuid::new_v4()));
    let area = codifix_config::AreaIdentityConfig {
        name: "synthetic".into(),
        instance_id: Uuid::new_v4(),
    };
    std::fs::write(&path,format!("[area]\nname='{}'\ninstance_id='{}'\n[auth]\npublic_base_url='https://inbox.example/'\n",area.name,area.instance_id)).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let mut config: codifix_config::AppConfig = serde_json::from_value(json!({
        "service": {"bind_addr":"127.0.0.1:4040","api_token":"ml_synthetic_only"},
        "database": {"url":"postgresql://invalid/synthetic"}
    }))
    .unwrap();
    config.resolved_config_path = Some(path.clone());
    config.area = Some(area);
    config.auth.public_base_url = Some("https://inbox.example/".into());
    assert!(InboxTarget::from_config(&config).is_ok());
    config.auth.public_base_url = Some("https://other.example/".into());
    assert!(InboxTarget::from_config(&config).is_err());
    config.auth.public_base_url = Some("https://inbox.example/".into());
    config.area.as_mut().unwrap().instance_id = Uuid::new_v4();
    assert!(InboxTarget::from_config(&config).is_err());
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn redirects_are_rejected_without_rendering_location_or_body() {
    let (inbox, m, server) = fixture().await;
    login(&inbox, &m).await;
    m.redirect_reads.store(true, Ordering::SeqCst);
    let error = inbox.list().await.unwrap_err();
    assert_eq!(error, "Inbox returned HTTP 302");
    assert!(!error.contains("127.0.0.1"));
    assert!(!error.contains("secret"));
    server.abort();
}
