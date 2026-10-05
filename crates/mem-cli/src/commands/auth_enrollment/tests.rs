// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use clap::Parser;
use credentials::sha256;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use uuid::Uuid;

#[derive(Default)]
struct FakeStore {
    items: RefCell<BTreeMap<(String, String), Vec<u8>>>,
    fail_write: bool,
}
impl CredentialStore for FakeStore {
    fn get(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .items
            .borrow()
            .get(&(service.into(), account.into()))
            .cloned())
    }
    fn put(&self, service: &str, account: &str, value: &[u8]) -> Result<()> {
        if self.fail_write {
            bail!("fake Keychain unavailable");
        }
        self.items
            .borrow_mut()
            .insert((service.into(), account.into()), value.to_vec());
        Ok(())
    }
    fn delete(&self, service: &str, account: &str) -> Result<()> {
        self.items
            .borrow_mut()
            .remove(&(service.into(), account.into()));
        Ok(())
    }
}
type Requests = Arc<Mutex<Vec<Vec<u8>>>>;
async fn server(
    responses: Vec<(u16, String, Option<String>)>,
) -> (CompanyOrigin, Requests, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin =
        CompanyOrigin::parse(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let task = tokio::spawn(async move {
        for (status, body, location) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut raw = Vec::new();
            loop {
                let mut chunk = [0u8; 4096];
                let count = socket.read(&mut chunk).await.unwrap();
                if count == 0 {
                    break;
                }
                raw.extend_from_slice(&chunk[..count]);
                if let Some(end) = raw.windows(4).position(|entry| entry == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&raw[..end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if raw.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            captured.lock().unwrap().push(raw);
            if status == 0 {
                continue;
            } // Simulate an accepted request with a lost reply.
            let location = location
                .map(|location| format!("Location: {location}\r\n"))
                .unwrap_or_default();
            let response = format!(
                "HTTP/1.1 {status} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{location}\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });
    (origin, requests, task)
}
fn body(request: &[u8]) -> serde_json::Value {
    let offset = request
        .windows(4)
        .position(|entry| entry == b"\r\n\r\n")
        .unwrap()
        + 4;
    serde_json::from_slice(&request[offset..]).unwrap()
}
fn login_args(origin: &CompanyOrigin) -> LoginArgs {
    LoginArgs {
        company: CompanyArgs {
            company: origin.0.clone(),
            allow_loopback: true,
        },
        name: "Test Mac".into(),
        no_browser: true,
    }
}
fn logout_args(origin: &CompanyOrigin) -> LogoutArgs {
    LogoutArgs {
        company: CompanyArgs {
            company: origin.0.clone(),
            allow_loopback: true,
        },
        forget_local_only: false,
        confirm: false,
    }
}

#[test]
fn origin_rejects_endpoint_credentials_and_implicit_insecure_targets() {
    for invalid in [
        "http://example.test",
        "https://user:secret@example.test",
        "https://example.test/project",
        "https://example.test/a/../",
        "https://example.test?token=private",
        "https://example.test#private",
        " https://example.test",
        "https://127.0.0.1",
        "http://127.0.0.1:4250",
    ] {
        assert!(CompanyOrigin::parse(invalid, false).is_err());
    }
    assert!(CompanyOrigin::parse("http://example.test", true).is_err());
    assert_eq!(
        CompanyOrigin::parse("HTTPS://EXAMPLE.TEST:443/", false)
            .unwrap()
            .0,
        "https://example.test"
    );
    assert_eq!(
        CompanyOrigin::parse("http://[::1]:4250", true).unwrap().0,
        "http://[::1]:4250"
    );
}

#[test]
fn cli_company_commands_require_exact_company_and_login_name() {
    use super::super::runtime::Cli;
    assert!(
        Cli::try_parse_from([
            "memory",
            "auth",
            "login",
            "--company",
            "https://example.test"
        ])
        .is_err()
    );
    let parsed = Cli::try_parse_from([
        "memory",
        "auth",
        "login",
        "--company",
        "https://example.test",
        "--name",
        "Mac",
        "--no-browser",
    ])
    .unwrap();
    assert!(matches!(
        parsed.command,
        Command::Auth(super::super::runtime::AuthArgs {
            command: AuthCommand::Login(_)
        })
    ));
    assert!(Cli::try_parse_from(["memory", "auth", "logout"]).is_err());
    assert!(
        Cli::try_parse_from([
            "memory",
            "auth",
            "logout",
            "--company",
            "https://example.test",
            "--forget-local-only"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "memory",
            "auth",
            "logout",
            "--company",
            "https://example.test",
            "--forget-local-only",
            "--confirm"
        ])
        .is_ok()
    );
    assert!(
        Cli::try_parse_from([
            "memory",
            "projects",
            "available",
            "--company",
            "https://example.test"
        ])
        .is_ok()
    );
}

#[test]
fn store_binds_company_and_device_and_resumes_exact_saved_secrets() {
    let store = FakeStore::default();
    let first = CompanyOrigin::parse("https://first.example.test", false).unwrap();
    let second = CompanyOrigin::parse("https://second.example.test", false).unwrap();
    let record = credentials::load_or_create(&store, &first, "Test Mac").unwrap();
    let resumed = credentials::load_or_create(&store, &first, "Test Mac").unwrap();
    assert_eq!(record.request_id, resumed.request_id);
    assert_eq!(record.device_id, resumed.device_id);
    assert_eq!(record.credential, resumed.credential);
    assert_eq!(record.poll_secret, resumed.poll_secret);
    assert_ne!(record.credential, record.poll_secret);
    assert!(credentials::load(&store, &second).unwrap().is_none());
    assert!(credentials::save(&store, &second, &record).is_err());
    assert!(credentials::load_or_create(&store, &first, "Other Mac").is_err());
    assert_eq!(store.items.borrow().len(), 2);
}

#[tokio::test]
async fn keychain_failure_prevents_the_first_external_start() {
    let (origin, requests, task) = server(vec![(200, "{}".into(), None)]).await;
    let store = FakeStore {
        fail_write: true,
        ..FakeStore::default()
    };
    assert!(login(&login_args(&origin), &store).await.is_err());
    assert!(requests.lock().unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn start_sends_hashes_only_and_does_not_inherit_owner_headers() {
    let (origin, requests, task) =
        server(vec![(500, "private provider detail".into(), None)]).await;
    let store = FakeStore::default();
    let record = credentials::load_or_create(&store, &origin, "Test Mac").unwrap();
    let error = transport::CompanyClient::new(origin)
        .unwrap()
        .start(&record)
        .await
        .err()
        .unwrap()
        .to_string();
    task.await.unwrap();
    assert!(error.contains("HTTP 500"));
    assert!(!error.contains("private provider detail"));
    let captured = requests.lock().unwrap();
    let headers = String::from_utf8_lossy(&captured[0]).to_lowercase();
    assert!(!headers.contains("authorization:"));
    assert!(!headers.contains("cookie:"));
    assert!(!headers.contains("x-api-token:"));
    let sent = body(&captured[0]);
    assert_eq!(sent["credential_sha256"], sha256(&record.credential));
    assert_eq!(sent["poll_sha256"], sha256(&record.poll_secret));
    let raw = serde_json::to_string(&sent).unwrap();
    assert!(!raw.contains(&record.credential));
    assert!(!raw.contains(&record.poll_secret));
}

#[tokio::test]
async fn redirects_are_not_followed_by_the_company_client() {
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (origin, _, task) = server(vec![(
        302,
        "{}".into(),
        Some(format!("http://{}/stolen", trap.local_addr().unwrap())),
    )])
    .await;
    let record = credentials::load_or_create(&FakeStore::default(), &origin, "Test Mac").unwrap();
    assert!(
        transport::CompanyClient::new(origin)
            .unwrap()
            .start(&record)
            .await
            .is_err()
    );
    task.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), trap.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn lost_poll_reply_retains_the_exact_replay_identity_and_credential() {
    let (origin, requests, task) = server(vec![(0, String::new(), None)]).await;
    let store = FakeStore::default();
    let record = credentials::load_or_create(&store, &origin, "Test Mac").unwrap();
    let error = transport::CompanyClient::new(origin.clone())
        .unwrap()
        .poll(&record)
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(!error.contains(&record.poll_secret));
    assert!(!error.contains(&record.credential));
    task.await.unwrap();
    let resumed = credentials::load_or_create(&store, &origin, "Test Mac").unwrap();
    assert_eq!(resumed.request_id, record.request_id);
    assert_eq!(resumed.device_id, record.device_id);
    assert_eq!(resumed.credential, record.credential);
    assert_eq!(
        body(&requests.lock().unwrap()[0])["poll_secret"],
        record.poll_secret
    );
}

#[tokio::test]
async fn mismatched_approval_device_does_not_activate_local_credential() {
    // Use a dedicated script after the record is saved so the receipt can bind
    // the start request but intentionally disagree on the approved device.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin =
        CompanyOrigin::parse(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap();
    let store = FakeStore::default();
    let record = credentials::load_or_create(&store, &origin, "Test Mac").unwrap();
    let request_id = record.request_id;
    let approval_url = transport::approval_url(&origin, "ABCD-1234").unwrap();
    let task = tokio::spawn(async move {
        for value in [
            serde_json::json!({"request_id":request_id,"user_code":"ABCD-1234","expires_at":"2099-01-01T00:00:00Z","poll_interval_seconds":2,"verification_uri_complete":approval_url}),
            serde_json::json!({"status":"approved","device_id":Uuid::new_v4(),"principal_id":Uuid::new_v4(),"projects":[]}),
        ] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 4096];
            let _ = socket.read(&mut request).await.unwrap();
            let body = value.to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    assert!(login(&login_args(&origin), &store).await.is_err());
    task.await.unwrap();
    assert!(
        !credentials::load(&store, &origin)
            .unwrap()
            .unwrap()
            .approved
    );
}

#[tokio::test]
async fn unavailable_logout_retains_credential_and_disables_local_project_use() {
    let (origin, requests, task) = server(vec![(503, "private failure".into(), None)]).await;
    let store = FakeStore::default();
    let mut record = credentials::load_or_create(&store, &origin, "Test Mac").unwrap();
    record.approved = true;
    credentials::save(&store, &origin, &record).unwrap();
    let args = CompanyArgs {
        company: origin.0.clone(),
        allow_loopback: true,
    };
    let error = logout(&logout_args(&origin), &store)
        .await
        .err()
        .unwrap()
        .to_string();
    task.await.unwrap();
    assert!(error.contains("logout is pending"));
    assert!(!error.contains(&record.credential));
    let saved = credentials::load(&store, &origin).unwrap().unwrap();
    assert!(saved.pending_logout);
    assert_eq!(saved.credential, record.credential);
    assert!(available(&args, &store).await.is_err());
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn pending_logout_requires_bound_cancel_proof_before_removing_local_secret() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin =
        CompanyOrigin::parse(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap();
    let store = FakeStore::default();
    let record = credentials::load_or_create(&store, &origin, "Test Mac").unwrap();
    let receipt = serde_json::json!({"status":"denied","request_id":record.request_id,"device_id":record.device_id}).to_string();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut raw = [0u8; 4096];
        let count = socket.read(&mut raw).await.unwrap();
        assert!(
            String::from_utf8_lossy(&raw[..count])
                .starts_with("POST /v1/onboarding/device/cancel ")
        );
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{receipt}",
                    receipt.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });
    logout(&logout_args(&origin), &store).await.unwrap();
    task.await.unwrap();
    assert!(credentials::load(&store, &origin).unwrap().is_none());
    assert!(store.items.borrow().is_empty());
}

#[tokio::test]
async fn lost_approval_cancel_conflict_falls_back_to_same_device_logout_proof() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin =
        CompanyOrigin::parse(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap();
    let store = FakeStore::default();
    let record = credentials::load_or_create(&store, &origin, "Test Mac").unwrap();
    let device_id = record.device_id;
    let credential = record.credential.clone();
    let task = tokio::spawn(async move {
        for status in [409, 200] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut raw = [0u8; 4096];
            let count = socket.read(&mut raw).await.unwrap();
            if status == 200 {
                assert!(
                    String::from_utf8_lossy(&raw[..count])
                        .starts_with("POST /v1/onboarding/device/logout ")
                );
                assert!(String::from_utf8_lossy(&raw[..count]).contains(&credential));
            }
            let body = if status == 200 {
                serde_json::json!({"logged_out":true,"device_id":device_id}).to_string()
            } else {
                "{}".into()
            };
            socket.write_all(format!("HTTP/1.1 {status} Synthetic\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    logout(&logout_args(&origin), &store).await.unwrap();
    task.await.unwrap();
    assert!(credentials::load(&store, &origin).unwrap().is_none());
}

#[test]
fn public_approval_url_never_accepts_secrets_or_another_origin() {
    let origin = CompanyOrigin::parse("https://company.example.test", false).unwrap();
    assert_eq!(
        transport::approval_url(&origin, "ABCD-1234").unwrap(),
        "https://company.example.test/?device_code=ABCD-1234"
    );
    assert!(transport::approval_url(&origin, "ABCD-1234&poll_secret=private").is_err());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn unsupported_native_store_fails_without_a_plaintext_fallback() {
    assert!(
        NativeKeychain
            .get("company", "device")
            .err()
            .unwrap()
            .to_string()
            .contains("no production credential store or plaintext fallback")
    );
}

#[tokio::test]
async fn unknown_start_cancel_keeps_pending_logout_until_explicit_local_only_recovery() {
    let (origin, requests, task) = server(vec![(401, "unknown proof".into(), None)]).await;
    let store = FakeStore::default();
    let record = credentials::load_or_create(&store, &origin, "Test Mac").unwrap();
    let args = logout_args(&origin);
    assert!(logout(&args, &store).await.is_err());
    task.await.unwrap();
    assert!(
        credentials::load(&store, &origin)
            .unwrap()
            .unwrap()
            .pending_logout
    );
    assert!(login(&login_args(&origin), &store).await.is_err());
    let mut recovery = logout_args(&origin);
    recovery.forget_local_only = true;
    assert!(logout(&recovery, &store).await.is_err());
    assert!(credentials::load(&store, &origin).unwrap().is_some());
    recovery.confirm = true;
    logout(&recovery, &store).await.unwrap();
    assert!(credentials::load(&store, &origin).unwrap().is_none());
    assert_eq!(requests.lock().unwrap().len(), 1);
    // New local preparation happens only after the explicit local-only reset.
    let next = credentials::load_or_create(&store, &origin, "Test Mac").unwrap();
    assert_ne!(next.device_id, record.device_id);
}

#[tokio::test]
async fn lost_logout_reply_replays_same_tombstone_proof_before_keychain_removal() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin =
        CompanyOrigin::parse(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap();
    let store = FakeStore::default();
    let mut record = credentials::load_or_create(&store, &origin, "Test Mac").unwrap();
    record.approved = true;
    credentials::save(&store, &origin, &record).unwrap();
    let receipt = serde_json::json!({"logged_out":true,"device_id":record.device_id}).to_string();
    let credential = record.credential.clone();
    let task = tokio::spawn(async move {
        for attempt in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut raw = [0u8; 4096];
            let count = socket.read(&mut raw).await.unwrap();
            assert!(String::from_utf8_lossy(&raw[..count]).contains(&credential));
            if attempt == 1 {
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{receipt}", receipt.len()).as_bytes()).await.unwrap();
            }
        }
    });
    assert!(logout(&logout_args(&origin), &store).await.is_err());
    assert!(
        credentials::load(&store, &origin)
            .unwrap()
            .unwrap()
            .pending_logout
    );
    logout(&logout_args(&origin), &store).await.unwrap();
    task.await.unwrap();
    assert!(credentials::load(&store, &origin).unwrap().is_none());
}
