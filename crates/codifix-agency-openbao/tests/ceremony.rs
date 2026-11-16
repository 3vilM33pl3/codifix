#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used)]
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use codifix_agency_openbao::{
    service::{self, AttestorConfig, InstallationRecord, NativeBinding, ProcessBinding},
    *,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tempfile::TempDir;
use tokio::task::JoinHandle;

const READER: &str = "reader-synthetic-token-000000000000000000000000";
const SHARE_A: &str = "syntheticShareAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const SHARE_B: &str = "syntheticShareBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
fn write(path: &Path, data: &[u8]) {
    protected::write(path, data).unwrap();
}
fn private(parent: &Path, name: &str) -> PathBuf {
    let d = parent.join(name);
    protected::create_private_directory(&d).unwrap();
    d
}
fn endpoint(address: std::net::SocketAddr) -> Endpoint {
    Endpoint {
        url: format!("http://{address}/"),
        ca_pem_file: None,
        fixture_localhost_http: true,
    }
}
async fn spawn(router: Router) -> (Endpoint, JoinHandle<()>) {
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ep = endpoint(tcp.local_addr().unwrap());
    (
        ep,
        tokio::spawn(async move {
            axum::serve(tcp, router).await.unwrap();
        }),
    )
}
fn native_fixture(
    parent: &Path,
    pid: u32,
    paths: Option<(PathBuf, PathBuf, PathBuf)>,
) -> (NativeBinding, PathBuf) {
    let (config, storage, pid_file) = paths.unwrap_or_else(|| {
        let config = parent.join("native-config.hcl");
        write(&config, b"fixture config");
        let storage = private(parent, "native-storage");
        let pid_file = parent.join("native.pid");
        write(&pid_file, pid.to_string().as_bytes());
        (config, storage, pid_file)
    });
    let installation = storage.join(".codifix-installation.json");
    (
        NativeBinding::inspect(pid, config, storage, installation).unwrap(),
        pid_file,
    )
}
#[derive(Clone)]
struct Mock {
    status: Arc<Mutex<SealStatus>>,
    submitted: Arc<Mutex<Vec<String>>>,
}
async fn status(State(m): State<Mock>) -> Json<SealStatus> {
    Json(m.status.lock().unwrap().clone())
}
#[derive(serde::Deserialize)]
struct Unseal {
    key: String,
}
async fn unseal(
    State(m): State<Mock>,
    Json(body): Json<Unseal>,
) -> std::result::Result<Json<SealStatus>, StatusCode> {
    if ![SHARE_A, SHARE_B].contains(&body.key.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    m.submitted.lock().unwrap().push(body.key);
    let mut s = m.status.lock().unwrap();
    s.progress += 1;
    s.nonce = "native-fresh-nonce".into();
    if s.progress >= s.t {
        s.sealed = false;
        s.progress = 0;
        s.nonce.clear();
    }
    Ok(Json(s.clone()))
}
fn initial_status() -> SealStatus {
    SealStatus {
        seal_type: "shamir".into(),
        initialized: true,
        sealed: true,
        t: 2,
        n: 3,
        progress: 0,
        nonce: String::new(),
        migration: false,
        recovery_seal: false,
    }
}
struct Fixture {
    _temp: TempDir,
    tasks: Vec<JoinHandle<()>>,
    controller: ControllerConfig,
    custodians: Vec<CustodianConfig>,
    identity_file: PathBuf,
    attestor: AttestorConfig,
    mock: Mock,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
impl Fixture {
    async fn new() -> Self {
        let mock = Mock {
            status: Arc::new(Mutex::new(initial_status())),
            submitted: Arc::new(Mutex::new(vec![])),
        };
        let (ep, task) = spawn(
            Router::new()
                .route("/v1/sys/seal-status", get(status))
                .route("/v1/sys/unseal", post(unseal))
                .with_state(mock.clone()),
        )
        .await;
        Self::with_target(
            ep,
            vec![task],
            mock,
            vec![SHARE_A.into(), SHARE_B.into()],
            std::process::id(),
            None,
        )
        .await
    }
    async fn with_target(
        ep: Endpoint,
        mut tasks: Vec<JoinHandle<()>>,
        mock: Mock,
        shares: Vec<String>,
        pid: u32,
        paths: Option<(PathBuf, PathBuf, PathBuf)>,
    ) -> Self {
        let temp = TempDir::new().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let control_dir = private(temp.path(), "controller");
        let attest_dir = private(temp.path(), "attestor");
        let (native, pid_file) = native_fixture(&attest_dir, pid, paths);
        let identity = Identity {
            installation_id: uuid::Uuid::new_v4().to_string(),
            key_generation: 1,
            native_binding: native.binding().unwrap(),
            seal_type: "shamir".into(),
            shares: 3,
            threshold: 2,
            valid_until: now() + 600,
        };
        protected::save(
            &native.installation_file,
            &InstallationRecord {
                installation_id: identity.installation_id.clone(),
                key_generation: identity.key_generation,
                maintenance: false,
            },
        )
        .unwrap();
        let identity_file = attest_dir.join("identity.json");
        protected::save(&identity_file, &identity).unwrap();
        let reader = attest_dir.join("reader");
        write(&reader, READER.as_bytes());
        let attestor = AttestorConfig {
            identity_file: identity_file.clone(),
            reader_token_file: reader,
            native,
            pid_file,
        };
        let (identity_endpoint, t) =
            spawn(service::attestor_router(attestor.clone()).unwrap()).await;
        tasks.push(t);
        let controller_reader = control_dir.join("reader");
        write(&controller_reader, READER.as_bytes());
        let target = Target {
            endpoint: ep,
            identity_endpoint,
            expected: identity,
            attestor_token_file: controller_reader,
        };
        let roster: Vec<_> = shares
            .iter()
            .enumerate()
            .map(|(i, s)| Member {
                custodian_id: format!("custodian-{i}"),
                share_id: format!("{:x}", Sha256::digest(s.as_bytes())),
                failure_domain: format!("host-{i}"),
            })
            .collect();
        let mut peers = vec![];
        let mut custodians = vec![];
        for (i, share) in shares.iter().enumerate() {
            let dir = private(temp.path(), &format!("custodian-{i}"));
            let token = format!("controller-to-custodian-{i}-synthetic-token-000000000000");
            let ctrl_token = control_dir.join(format!("token-{i}"));
            write(&ctrl_token, token.as_bytes());
            let own_token = dir.join("controller-token");
            write(&own_token, token.as_bytes());
            let own_reader = dir.join("reader");
            write(&own_reader, READER.as_bytes());
            let share_file = dir.join("share");
            write(&share_file, share.as_bytes());
            let mut own_target = target.clone();
            own_target.attestor_token_file = own_reader;
            let c = CustodianConfig {
                target: own_target,
                roster: roster.clone(),
                custodian_id: format!("custodian-{i}"),
                share_file,
                controller_token_file: own_token,
                state_file: dir.join("state.json"),
            };
            protected::initialize(
                &c.state_file,
                &CustodianState {
                    binding: c.target.binding().unwrap(),
                    ..Default::default()
                },
            )
            .unwrap();
            let (endpoint, t) = spawn(service::custodian_router(c.clone()).unwrap()).await;
            tasks.push(t);
            peers.push(Peer {
                custodian_id: c.custodian_id.clone(),
                endpoint,
                controller_token_file: ctrl_token,
            });
            custodians.push(c);
        }
        let controller = ControllerConfig {
            target,
            roster,
            peers,
            state_file: control_dir.join("state.json"),
        };
        protected::initialize(
            &controller.state_file,
            &ControllerState {
                binding: controller.target.binding().unwrap(),
                ..Default::default()
            },
        )
        .unwrap();
        Self {
            _temp: temp,
            tasks,
            controller,
            custodians,
            identity_file,
            attestor,
            mock,
        }
    }
    fn request(&self, sequence: u64) -> Submission {
        Submission {
            ceremony: Ceremony {
                challenge: uuid::Uuid::new_v4().to_string(),
                sequence,
                issued_at: now(),
                expires_at: now() + 100,
                target_binding: self.controller.target.binding().unwrap(),
                selected: self
                    .controller
                    .roster
                    .iter()
                    .map(|m| m.custodian_id.clone())
                    .collect(),
            },
            expected_progress: 0,
            expected_nonce: String::new(),
        }
    }
}

#[tokio::test]
async fn quorum_direct_submission_and_safe_observation() {
    let f = Fixture::new().await;
    let mut events = vec![];
    let s = transport::controller_tick(&f.controller, |o| events.push(o))
        .await
        .unwrap();
    assert!(!s.sealed);
    assert_eq!(f.mock.submitted.lock().unwrap().len(), 2);
    let serialized = serde_json::to_string(&events).unwrap();
    for secret in [SHARE_A, SHARE_B, READER] {
        assert!(!serialized.contains(secret));
    }
    assert!(events.iter().any(|e| matches!(e, Observation::Unsealed)));
    assert_eq!(
        protected::load::<ControllerState>(&f.controller.state_file)
            .unwrap()
            .sequence,
        1
    );
    // A routine reseal creates a new challenge/sequence, independently of any DB.
    *f.mock.status.lock().unwrap() = initial_status();
    transport::controller_tick(&f.controller, |_| {})
        .await
        .unwrap();
    assert_eq!(
        protected::load::<ControllerState>(&f.controller.state_file)
            .unwrap()
            .sequence,
        2
    );
}
#[tokio::test]
async fn replay_survives_restart_expired_and_cross_target_fail_closed() {
    let f = Fixture::new().await;
    let request = f.request(1);
    transport::custodian_submit(&f.custodians[0], &request)
        .await
        .unwrap();
    assert!(matches!(
        transport::custodian_submit(&f.custodians[0], &request).await,
        Err(Error::Ceremony)
    ));
    let reloaded: CustodianState = protected::load(&f.custodians[0].state_file).unwrap();
    assert_eq!(reloaded.last_sequence, 1);
    let mut expired = f.request(2);
    expired.ceremony.expires_at = now();
    assert!(
        transport::custodian_submit(&f.custodians[0], &expired)
            .await
            .is_err()
    );
    let mut changed = f.request(3);
    changed.ceremony.target_binding = "unexpected-target".into();
    assert!(
        transport::custodian_submit(&f.custodians[0], &changed)
            .await
            .is_err()
    );
    assert_eq!(f.mock.submitted.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn changed_identity_generation_seal_and_missing_state_never_submit() {
    for field in [
        "installation",
        "generation",
        "binding",
        "expired",
        "migration",
        "threshold",
        "missing_state",
    ] {
        let f = Fixture::new().await;
        let mut i: Identity = protected::load(&f.identity_file).unwrap();
        match field {
            "installation" => i.installation_id = "different-installation".into(),
            "generation" => i.key_generation += 1,
            "binding" => i.native_binding = "different-native-binding".into(),
            "expired" => i.valid_until = now(),
            "migration" => f.mock.status.lock().unwrap().migration = true,
            "threshold" => f.mock.status.lock().unwrap().t = 1,
            _ => fs::remove_file(&f.controller.state_file).unwrap(),
        }
        protected::save(&f.identity_file, &i).unwrap();
        let mut events = vec![];
        assert!(
            transport::controller_tick(&f.controller, |o| events.push(o))
                .await
                .is_err(),
            "{field}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Observation::Escalation { .. }))
        );
        assert!(f.mock.submitted.lock().unwrap().is_empty());
    }
}
#[tokio::test]
async fn endpoint_authentication_and_scoped_tokens() {
    let f = Fixture::new().await;
    let http = reqwest::Client::new();
    let url = format!("{}v1/ceremony", f.controller.peers[0].endpoint.url);
    for token in [READER, "invalid-synthetic-token-000000000000000000000000"] {
        let r = http
            .post(&url)
            .bearer_auth(token)
            .json(&f.request(1))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 401);
    }
    let other = protected::read(&f.controller.peers[1].controller_token_file).unwrap();
    assert_eq!(
        http.post(&url)
            .bearer_auth(std::str::from_utf8(&other).unwrap())
            .json(&f.request(1))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert!(f.mock.submitted.lock().unwrap().is_empty());
}
#[tokio::test]
async fn interrupted_partial_native_ceremony_and_missing_share_stop_without_retry() {
    let f = Fixture::new().await;
    fs::remove_file(&f.custodians[1].share_file).unwrap();
    assert!(
        transport::controller_tick(&f.controller, |_| {})
            .await
            .is_err()
    );
    assert_eq!(f.mock.submitted.lock().unwrap().len(), 1);
    assert!(matches!(
        transport::controller_tick(&f.controller, |_| {}).await,
        Err(Error::Incomplete)
    ));
    assert_eq!(f.mock.submitted.lock().unwrap().len(), 1);
    let native = Fixture::new().await;
    native.mock.status.lock().unwrap().progress = 1;
    assert!(matches!(
        transport::controller_tick(&native.controller, |_| {}).await,
        Err(Error::Incomplete)
    ));
    assert!(native.mock.submitted.lock().unwrap().is_empty());
}
#[tokio::test]
async fn duplicate_roster_and_share_mismatch_rejected_independently() {
    let f = Fixture::new().await;
    let mut c = f.custodians[0].clone();
    for field in ["id", "share", "domain"] {
        let mut roster = c.roster.clone();
        match field {
            "id" => roster[1].custodian_id = roster[0].custodian_id.clone(),
            "share" => roster[1].share_id = roster[0].share_id.clone(),
            _ => roster[1].failure_domain = roster[0].failure_domain.clone(),
        };
        c.roster = roster;
        assert!(c.validate(now()).is_err());
        c = f.custodians[0].clone();
    }
    write(&c.share_file, SHARE_B.as_bytes());
    assert!(matches!(
        transport::custodian_submit(&c, &f.request(1)).await,
        Err(Error::Configuration)
    ));
    assert!(f.mock.submitted.lock().unwrap().is_empty());
}
#[tokio::test]
async fn native_nonce_reset_refuses_second_custodian() {
    let f = Fixture::new().await;
    transport::custodian_submit(&f.custodians[0], &f.request(1))
        .await
        .unwrap();
    let mut next = f.request(1);
    next.expected_progress = 1;
    next.expected_nonce = "wrong-native-nonce".into();
    assert!(matches!(
        transport::custodian_submit(&f.custodians[1], &next).await,
        Err(Error::Ceremony)
    ));
    assert_eq!(f.mock.submitted.lock().unwrap().len(), 1);
}
#[test]
fn private_files_fail_on_permissions_symlinks_hardlinks_and_lock_contention() {
    let temp = TempDir::new().unwrap();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let p = temp.path().join("private");
    write(&p, b"synthetic");
    let lock = protected::StateLock::acquire(&p).unwrap();
    assert!(matches!(
        protected::StateLock::acquire(&p),
        Err(Error::Busy)
    ));
    drop(lock);
    fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(protected::read(&p).is_err());
    fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
    let hard = temp.path().join("hard");
    fs::hard_link(&p, &hard).unwrap();
    assert!(protected::read(&p).is_err());
    fs::remove_file(hard).unwrap();
    let symlink = temp.path().join("symlink");
    std::os::unix::fs::symlink(&p, &symlink).unwrap();
    assert!(protected::read(&symlink).is_err());
    assert!(protected::initialize(&p, &ControllerState::default()).is_err());
}
#[test]
fn production_transport_rejects_http_redirectable_origins_and_missing_pin() {
    for url in [
        "http://example.com/",
        "http://localhost/",
        "http://127.0.0.2/",
        "https://example.com/",
        "http://127.0.0.1/?q=secret",
        "http://user@127.0.0.1/",
        "http://127.0.0.1/prefix",
    ] {
        assert!(
            Endpoint {
                url: url.into(),
                ca_pem_file: None,
                fixture_localhost_http: true
            }
            .validate()
            .is_err()
        );
    }
}

struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[tokio::test]
async fn attestor_detects_real_process_restart_or_exit() {
    let mut process = Child(
        std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap(),
    );
    let binding = ProcessBinding::inspect(process.0.id()).unwrap();
    binding.verify().unwrap();
    process.0.kill().unwrap();
    process.0.wait().unwrap();
    assert!(binding.verify().is_err());
}

/// Explicit opt-in: synthetic, isolated file-storage OpenBao; never contacts a
/// production URL or reads credentials. Test bootstrap owns only generated shares.
#[tokio::test]
#[ignore = "requires local bao executable; creates isolated synthetic vault"]
async fn actual_openbao_shamir_two_of_three() {
    let dir = TempDir::new().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    drop(socket);
    let config = dir.path().join("bao.hcl");
    let storage = dir.path().join("data");
    protected::create_private_directory(&storage).unwrap();
    let native_pid_file = dir.path().join("native.pid");
    write(&config,format!("disable_mlock = true\npid_file = \"{}\"\napi_addr = \"http://{address}\"\nlistener \"tcp\" {{\n address = \"{address}\"\n tls_disable = true\n}}\nstorage \"file\" {{\n path = \"{}\"\n}}\n",native_pid_file.display(),storage.display()).as_bytes());
    let executable = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|p| p.join("bao"))
        .find(|p| p.is_file())
        .expect("bao executable on PATH");
    let mut child = Child(
        std::process::Command::new(&executable)
            .arg("server")
            .arg("-config")
            .arg(&config)
            .env_clear()
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let http = reqwest::Client::new();
    let base = format!("http://{address}");
    let mut ready = false;
    for _ in 0..100 {
        if http
            .get(format!("{base}/v1/sys/seal-status"))
            .send()
            .await
            .is_ok()
        {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    assert!(ready);
    let mut init: serde_json::Value = http
        .post(format!("{base}/v1/sys/init"))
        .json(&serde_json::json!({"secret_shares":3,"secret_threshold":2}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let shares: Vec<String> = init["keys_base64"]
        .as_array_mut()
        .unwrap()
        .drain(..2)
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    // Bootstrap root token and unused synthetic share are discarded; no later role uses them.
    init = serde_json::Value::Null;
    assert!(init.is_null());
    let mock = Mock {
        status: Arc::new(Mutex::new(initial_status())),
        submitted: Arc::new(Mutex::new(vec![])),
    };
    let mut f = Fixture::with_target(
        endpoint(address),
        vec![],
        mock,
        shares,
        child.0.id(),
        Some((config.clone(), storage.clone(), native_pid_file.clone())),
    )
    .await;
    // Exercise the real custodian executable in separate processes. This fixture
    // still shares a test account; independent production hosts remain untested.
    let mut workers = vec![];
    for (index, c) in f.custodians.iter().enumerate() {
        let directory = c.state_file.parent().unwrap();
        let config_file = directory.join("config.json");
        let listener_file = directory.join("listener.json");
        protected::save(&config_file, c).unwrap();
        let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = tcp.local_addr().unwrap();
        drop(tcp);
        let listener = service::Listener {
            address,
            tls_cert_file: None,
            tls_key_file: None,
            fixture_localhost_http: true,
        };
        protected::save(&listener_file, &listener).unwrap();
        workers.push(Child(
            std::process::Command::new(env!("CARGO_BIN_EXE_codifix-agency-openbao"))
                .arg("custodian-serve")
                .arg(&config_file)
                .arg(&listener_file)
                .env_clear()
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        ));
        f.controller.peers[index].endpoint = endpoint(address);
        let mut ready = false;
        for _ in 0..100 {
            if http
                .get(format!("http://{address}/v1/ceremony"))
                .send()
                .await
                .is_ok()
            {
                ready = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(ready);
    }
    let config_file = f
        .controller
        .state_file
        .parent()
        .unwrap()
        .join("config.json");
    protected::save(&config_file, &f.controller).unwrap();
    let status = tokio::process::Command::new(env!("CARGO_BIN_EXE_codifix-agency-openbao"))
        .arg("status")
        .arg(&config_file)
        .env_clear()
        .output()
        .await
        .unwrap();
    assert!(status.status.success());
    assert!(
        serde_json::from_slice::<SealStatus>(&status.stdout)
            .unwrap()
            .sealed
    );
    let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_codifix-agency-openbao"))
        .arg("controller-once")
        .arg(&config_file)
        .env_clear()
        .output()
        .await
        .unwrap();
    assert!(result.status.success());
    let events: Vec<Observation> = std::str::from_utf8(&result.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Observation::CustodianSubmitted { .. }))
            .count(),
        2
    );
    assert!(
        !transport::verified_status(&f.controller.target)
            .await
            .unwrap()
            .sealed
    );
    for c in &f.custodians {
        assert_eq!(
            protected::load::<CustodianState>(&c.state_file)
                .unwrap()
                .last_sequence,
            1
        );
    }
    let original_process = ProcessBinding::inspect(child.0.id()).unwrap();
    let original_binding = f.controller.target.binding().unwrap();
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert!(
        transport::verified_status(&f.controller.target)
            .await
            .is_err()
    );
    // Ordinary native restart: OpenBao maintains its own protected pid_file.
    // No operator updates enrollment, identities, generations or replay records.
    child = Child(
        std::process::Command::new(&executable)
            .arg("server")
            .arg("-config")
            .arg(&config)
            .env_clear()
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut ready = false;
    for _ in 0..100 {
        if http
            .get(format!("{base}/v1/sys/seal-status"))
            .send()
            .await
            .is_ok()
        {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    assert!(ready);
    assert_eq!(
        protected::read(&native_pid_file).unwrap().as_slice(),
        child.0.id().to_string().as_bytes()
    );
    let restarted = ProcessBinding::inspect(child.0.id()).unwrap();
    assert_ne!(original_process.marker, restarted.marker);
    assert!(
        transport::verified_status(&f.controller.target)
            .await
            .unwrap()
            .sealed
    );
    let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_codifix-agency-openbao"))
        .arg("controller-once")
        .arg(&config_file)
        .env_clear()
        .output()
        .await
        .unwrap();
    assert!(
        result.status.success(),
        "controller restart recovery failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        !transport::verified_status(&f.controller.target)
            .await
            .unwrap()
            .sealed
    );
    assert_eq!(f.controller.target.binding().unwrap(), original_binding);
    assert_eq!(
        protected::load::<ControllerState>(&f.controller.state_file)
            .unwrap()
            .sequence,
        2
    );
    for c in &f.custodians {
        assert_eq!(
            protected::load::<CustodianState>(&c.state_file)
                .unwrap()
                .last_sequence,
            2
        );
    }
}

#[tokio::test]
async fn https_server_requires_enrolled_ca_and_does_not_follow_redirects() {
    let dir = TempDir::new().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let pem = cert.cert.pem();
    let cert_file = dir.path().join("cert.pem");
    write(&cert_file, pem.as_bytes());
    let key_file = dir.path().join("key.pem");
    write(&key_file, cert.key_pair.serialize_pem().as_bytes());
    let wrong = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let wrong_file = dir.path().join("wrong.pem");
    write(&wrong_file, wrong.cert.pem().as_bytes());
    let (native, pid_file) = native_fixture(dir.path(), std::process::id(), None);
    let identity = Identity {
        installation_id: uuid::Uuid::new_v4().to_string(),
        key_generation: 1,
        native_binding: native.binding().unwrap(),
        seal_type: "shamir".into(),
        shares: 3,
        threshold: 2,
        valid_until: now() + 600,
    };
    protected::save(
        &native.installation_file,
        &InstallationRecord {
            installation_id: identity.installation_id.clone(),
            key_generation: identity.key_generation,
            maintenance: false,
        },
    )
    .unwrap();
    let identity_file = dir.path().join("identity.json");
    protected::save(&identity_file, &identity).unwrap();
    let reader = dir.path().join("reader");
    write(&reader, READER.as_bytes());
    let config = AttestorConfig {
        identity_file,
        reader_token_file: reader.clone(),
        native,
        pid_file,
    };
    let mock = Mock {
        status: Arc::new(Mutex::new(initial_status())),
        submitted: Arc::new(Mutex::new(vec![])),
    };
    let router = service::attestor_router(config).unwrap().merge(
        Router::new()
            .route("/v1/sys/seal-status", get(status))
            .with_state(mock),
    );
    let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = tcp.local_addr().unwrap();
    drop(tcp);
    let listener = service::Listener {
        address,
        tls_cert_file: Some(cert_file.clone()),
        tls_key_file: Some(key_file),
        fixture_localhost_http: false,
    };
    let task = tokio::spawn(service::serve(router, listener));
    let endpoint = Endpoint {
        url: format!("https://{address}/"),
        ca_pem_file: Some(cert_file),
        fixture_localhost_http: false,
    };
    let target = Target {
        endpoint: endpoint.clone(),
        identity_endpoint: endpoint,
        expected: identity,
        attestor_token_file: reader,
    };
    let mut ready = false;
    for _ in 0..50 {
        if transport::verified_status(&target).await.is_ok() {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(ready);
    let mut untrusted = target.clone();
    untrusted.endpoint.ca_pem_file = Some(wrong_file.clone());
    untrusted.identity_endpoint.ca_pem_file = Some(wrong_file);
    assert!(transport::verified_status(&untrusted).await.is_err());
    // Changing the enrolled trust anchor also changes the persisted ceremony binding.
    assert_ne!(target.binding().unwrap(), untrusted.binding().unwrap());
    task.abort();
    let f = Fixture::new().await;
    let router = Router::new().route(
        "/codifix-agency/v1/identity",
        get(|| async { axum::response::Redirect::temporary("http://127.0.0.1:1/leak") }),
    );
    let (ep, task) = spawn(router).await;
    let mut target = f.controller.target.clone();
    target.identity_endpoint = ep;
    assert!(transport::verified_status(&target).await.is_err());
    task.abort();
}

#[tokio::test]
async fn stable_installation_guards_unknown_config_storage_generation_and_maintenance() {
    for field in [
        "config",
        "storage",
        "installation",
        "generation",
        "maintenance",
        "pid",
        "binary",
    ] {
        let f = Fixture::new().await;
        match field {
            "config" => write(
                &f.attestor.native.config_file,
                b"unexpected native configuration",
            ),
            "storage" => {
                let storage = &f.attestor.native.storage_directory;
                let moved = storage.with_extension("old");
                fs::rename(storage, &moved).unwrap();
                protected::create_private_directory(storage).unwrap();
                let record = protected::read(&moved.join(".codifix-installation.json")).unwrap();
                write(&f.attestor.native.installation_file, &record);
            }
            "pid" => write(&f.attestor.pid_file, b"4294967295"),
            "binary" => {
                let mut wrong = f.attestor.native.clone();
                wrong.executable_sha256 = "0".repeat(64);
                assert!(wrong.verify(std::process::id()).is_err());
                continue;
            }
            _ => {
                let mut record: InstallationRecord =
                    protected::load(&f.attestor.native.installation_file).unwrap();
                match field {
                    "installation" => record.installation_id = "unknown-installation".into(),
                    "generation" => record.key_generation += 1,
                    _ => record.maintenance = true,
                };
                protected::save(&f.attestor.native.installation_file, &record).unwrap();
            }
        }
        assert!(
            transport::controller_tick(&f.controller, |_| {})
                .await
                .is_err(),
            "{field}"
        );
        assert!(f.mock.submitted.lock().unwrap().is_empty());
    }
}
#[tokio::test]
async fn fresh_challenge_and_process_attestation_expiry_are_verified() {
    let f = Fixture::new().await;
    let attestation = f.attestor.attest(uuid::Uuid::new_v4().to_string()).unwrap();
    assert_eq!(attestation.identity, f.controller.target.expected);
    for field in ["challenge", "expired", "process"] {
        let mut reply = attestation.clone();
        match field {
            "challenge" => reply.challenge = uuid::Uuid::new_v4().to_string(),
            "expired" => reply.expires_at = now(),
            _ => reply.process.marker.clear(),
        };
        let (ep, task) = spawn(Router::new().route(
            "/codifix-agency/v1/identity",
            get(
                move |axum::extract::Query(query): axum::extract::Query<
                    std::collections::HashMap<String, String>,
                >| {
                    let mut reply = reply.clone();
                    if field != "challenge" {
                        reply.challenge = query.get("challenge").cloned().unwrap_or_default();
                    }
                    async move { Json(reply) }
                },
            ),
        ))
        .await;
        let mut target = f.controller.target.clone();
        target.identity_endpoint = ep;
        assert!(transport::verified_status(&target).await.is_err());
        task.abort();
    }
}
