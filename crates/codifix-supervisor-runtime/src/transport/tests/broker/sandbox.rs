//! Explicitly invoked synthetic rehearsal: authentic patched Pi, network-none
//! container, private Unix broker, fake provider and disposable PostgreSQL.
use super::*;
use std::{
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    process::Stdio,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const IMAGE: &str =
    "ubuntu@sha256:cd21a4f68a617580279d4b091cb18e3af9fa8a87500665f0ae5f7f757d17d367";

#[tokio::test]
#[ignore = "requires explicit patched Pi and freshly built sandbox driver paths, pinned local Docker image and required disposable PostgreSQL"]
async fn patched_pi_reaches_durable_broker_only_through_private_socket() {
    run(Mode::Good).await;
    run(Mode::RevokeDuringCount).await;
    run(Mode::MalformedUsage).await;
}

async fn run(mode: Mode) {
    assert_eq!(std::env::var("CODIFIX_TEST_REQUIRE_DB").as_deref(), Ok("1"));
    let artifact = PathBuf::from(
        std::env::var_os("CODIFIX_PI_PATCHED_ARTIFACT").expect("explicit pinned artifact"),
    );
    let driver = PathBuf::from(
        std::env::var_os("CODIFIX_PI_SANDBOX_DRIVER").expect("explicit freshly built driver"),
    );
    for path in [&artifact, &driver] {
        assert!(path.is_absolute() && path.is_file());
        assert!(!path.to_string_lossy().contains(','));
        assert!(
            !std::fs::symlink_metadata(path)
                .expect("file")
                .file_type()
                .is_symlink()
        );
    }
    let (db, store, lease, revision, fake, client, provider) =
        fixture(mode).await.expect("required database");
    let grant = crate::transport::tests::disclosure::grant(&db, &fake).await;
    let disclosure = Arc::new(crate::local_disclosure::LocalDisclosure::new(
        db.pool.clone(),
        grant,
        "Review this synthetic approved development note".into(),
    ));
    let request = Arc::new(
        disclosure
            .prepare()
            .await
            .expect("current source and disclosure grant"),
    );
    *fake.expected.write().expect("expected") = Arc::clone(&request);
    let work = Arc::new(crate::pi_broker::GuardedBrokerJob {
        store,
        lease,
        execution: fake.execution(revision, Uuid::new_v4()),
        request: Arc::clone(&request),
        transport: client,
        disclosure,
    });
    let (broker, token) =
        PiBroker::new(Arc::clone(&request), work.clone(), Duration::from_secs(120))
            .expect("broker");
    let directory = tempfile::Builder::new()
        .prefix("memory-pi-broker-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in("/data/tmp")
        .expect("private Unix socket parent");
    let uid = directory.path().metadata().expect("private owner").uid();
    assert_ne!(uid, 0);
    let listener =
        tokio::net::UnixListener::bind(directory.path().join("job.sock")).expect("socket");
    std::fs::set_permissions(
        directory.path().join("job.sock"),
        std::fs::Permissions::from_mode(0o600),
    )
    .expect("socket mode");
    let server = tokio::spawn(async move {
        axum::serve(listener, broker.router())
            .await
            .expect("Unix broker");
    });
    let name = format!("memory-pi-broker-test-{}", Uuid::new_v4().simple());
    let mut command = tokio::process::Command::new("/usr/bin/docker");
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args([
            "run",
            "--rm",
            "--pull=never",
            "--network=none",
            "--read-only",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--pids-limit=32",
            "--memory=512m",
            "--cpus=1",
            "--interactive",
            "--name",
            &name,
        ])
        .arg(format!("--user={uid}:{uid}"))
        .arg(format!(
            "--tmpfs=/scratch:rw,exec,nosuid,nodev,size=160m,mode=0700,uid={uid},gid={uid}"
        ))
        .arg(format!(
            "--mount=type=bind,src={},dst=/pi/pi,readonly",
            artifact.display()
        ))
        .arg(format!(
            "--mount=type=bind,src={},dst=/driver,readonly",
            driver.display()
        ))
        .arg(format!(
            "--mount=type=bind,src={},dst=/broker,readonly",
            directory.path().display()
        ))
        .args([IMAGE, "/driver"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().expect("isolated child");
    let admission = crate::pi_admission::PiAdmission::new(&request);
    let input = serde_json::to_vec(&json!({"system":admission.system_prompt(), "prompt":admission.prompt(), "broker_token":token.bearer_for_child()})).expect("private input");
    let result = tokio::time::timeout(Duration::from_secs(135), async {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(&input).await.expect("input");
        drop(stdin);
        let mut out = child.stdout.take().expect("stdout").take(8193);
        let mut err = child.stderr.take().expect("stderr").take(8193);
        let mut bytes = Vec::new();
        let mut errors = Vec::new();
        let (a, b, status) = tokio::join!(
            out.read_to_end(&mut bytes),
            err.read_to_end(&mut errors),
            child.wait()
        );
        a.expect("bounded stdout");
        b.expect("bounded stderr");
        (status.expect("reap"), bytes, errors.len())
    })
    .await;
    if result.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
        // Only our uniquely named disposable container; never broad cleanup.
        let _ = tokio::process::Command::new("/usr/bin/docker")
            .args(["rm", "--force", &name])
            .output()
            .await;
    }
    server.abort();
    provider.abort();
    let (status, bytes, error_bytes) = result.expect("bounded container deadline");
    assert!(
        bytes.len() <= 8192 && error_bytes <= 8192,
        "metadata-only output bounds"
    );
    // Do not put raw child output in a panic (it is an untrusted boundary).
    let report: Value = serde_json::from_slice(&bytes).expect("typed child metadata");
    assert_eq!(
        report["process"], "reaped",
        "child failure class: {}",
        report["failure"]
    );
    assert_eq!(fake.count.load(Ordering::SeqCst), 1);
    if matches!(mode, Mode::Good) {
        assert!(
            status.success(),
            "child failure class: {}",
            report["failure"]
        );
        assert_eq!(report["failure"], Value::Null);
        let proposal = work
            .store
            .private_proposal(
                Area::Workshop,
                work.execution.project_id,
                work.lease.action_id(),
            )
            .await
            .expect("saved proposal");
        let digest: [u8; 32] =
            Sha256::digest(serde_json::to_vec(&proposal.proposal).expect("proposal")).into();
        assert_eq!(report["proposal_sha256"], json!(digest));
        assert_eq!(
            work.store
                .settled_usage(work.execution.attempt_id)
                .await
                .expect("usage")
                .expect("known")
                .total_tokens,
            120
        );
        assert_eq!(
            work.store
                .action(Area::Workshop, work.lease.action_id())
                .await
                .expect("action")
                .state,
            ActionState::Verified
        );
        assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    } else {
        assert!(!status.success());
        assert!(report["failure"].is_object());
        assert_eq!(report["proposal_sha256"], Value::Null);
        assert!(
            work.store
                .private_proposal(
                    Area::Workshop,
                    work.execution.project_id,
                    work.lease.action_id()
                )
                .await
                .is_err()
        );
        assert_eq!(
            fake.generated.load(Ordering::SeqCst),
            usize::from(matches!(mode, Mode::MalformedUsage))
        );
        if matches!(mode, Mode::MalformedUsage) {
            assert_eq!(
                work.store
                    .attempt(work.execution.attempt_id)
                    .await
                    .expect("retained reservation")
                    .state,
                AttemptState::Uncertain
            );
        }
    }
    db.cleanup().await.expect("cleanup");
}
