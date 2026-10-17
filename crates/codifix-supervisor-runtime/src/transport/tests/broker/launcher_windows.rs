//! Explicit native rehearsal: real pinned Pi/driver, actual production broker,
//! disposable PostgreSQL and synthetic fake provider only. No network in Pi.
use super::*;
use crate::launcher::{self, RunControl, SandboxConfig};
use std::{collections::BTreeSet, path::PathBuf};

#[tokio::test]
#[ignore = "requires explicit protected Windows Pi sandbox config/hash and disposable PostgreSQL; no paid providers"]
async fn native_production_launcher_delivers_revokes_accounts_and_recovers() {
    run(Mode::Good, false).await;
    run(Mode::RevokeDuringCount, false).await;
    run(Mode::MalformedUsage, false).await;
    run(Mode::Good, true).await;
}

fn configured() -> SandboxConfig {
    assert_eq!(std::env::var("CODIFIX_TEST_REQUIRE_DB").as_deref(), Ok("1"));
    let path = PathBuf::from(
        std::env::var_os("CODIFIX_PI_WINDOWS_SANDBOX_CONFIG")
            .expect("explicit private synthetic sandbox config"),
    );
    let digest = std::env::var("CODIFIX_PI_WINDOWS_SANDBOX_CONFIG_SHA256")
        .expect("exact operator-reviewed sandbox config hash");
    let bytes =
        crate::private_files::read(&path, &digest).expect("protected hash-bound sandbox config");
    let config: SandboxConfig = serde_json::from_slice(&bytes).expect("typed sandbox metadata");
    for path in [&config.pi, &config.driver, &config.private_runtime] {
        assert!(path.is_absolute());
    }
    config
        .windows_artifacts
        .as_ref()
        .expect("native pinned Pi provenance")
        .validate()
        .expect("unchanged source and patch pins");
    config
}
fn entries(path: &std::path::Path) -> BTreeSet<std::ffi::OsString> {
    std::fs::read_dir(path)
        .expect("private synthetic runtime")
        .map(|entry| entry.expect("runtime entry").file_name())
        .collect()
}

async fn run(mode: Mode, cancel: bool) {
    let config = configured();
    let before = entries(&config.private_runtime);
    let runtime = config.private_runtime.clone();
    let (db, store, lease, revision, fake, client, provider) =
        fixture(mode).await.expect("required disposable database");
    let grant = crate::transport::tests::disclosure::grant(&db, &fake).await;
    let control = RunControl::default();
    let disclosure = Arc::new(
        crate::local_disclosure::LocalDisclosure::new(
            db.pool.clone(),
            grant,
            "Review this synthetic approved development note".into(),
        )
        .with_control(control.clone()),
    );
    let request = Arc::new(
        disclosure
            .prepare()
            .await
            .expect("current synthetic source and disclosure grant"),
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
    let pause = Arc::new(tokio::sync::Notify::new());
    if cancel {
        *fake.generation_pause.lock().expect("pause") = Some(Arc::clone(&pause));
    }
    let launched = tokio::spawn(launcher::launch(
        config,
        request,
        work.clone(),
        control.clone(),
    ));
    if cancel {
        tokio::time::timeout(Duration::from_secs(30), async {
            while fake.generated.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fake provider started");
        control.stop();
        pause.notify_one();
    }
    let result = tokio::time::timeout(Duration::from_secs(140), launched)
        .await
        .expect("bounded native cleanup")
        .expect("launcher joined");
    let after = entries(&runtime);
    let retained: Vec<_> = after.difference(&before).collect();
    if result.is_err()
        && record_failure_metadata(&runtime, &retained, result, &fake, cancel).is_err()
    {
        eprintln!("protected rehearsal metadata unavailable; original launcher result follows");
    }
    // Report the actual typed launcher failure before a secondary zero-call
    // assertion can hide the cause. No raw child/provider output is printed.
    if matches!(mode, Mode::Good) && !cancel {
        assert!(
            result.is_ok(),
            "native production launcher failed before delivery: {result:?}; inspect protected rehearsal metadata"
        );
    }
    assert_eq!(fake.count.load(Ordering::SeqCst), 1);
    if matches!(mode, Mode::Good) && !cancel {
        let digest = result.expect("native production launcher delivered");
        let proposal = work
            .store
            .private_proposal(
                Area::Workshop,
                work.execution.project_id,
                work.lease.action_id(),
            )
            .await
            .expect("durable proposal");
        let expected: [u8; 32] =
            Sha256::digest(serde_json::to_vec(&proposal.proposal).expect("proposal")).into();
        assert_eq!(digest, expected);
        assert_eq!(
            work.store
                .settled_usage(work.execution.attempt_id)
                .await
                .expect("settlement")
                .expect("known usage")
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
        assert_eq!(
            entries(&runtime),
            before,
            "successful sandbox files reclaimed only after verified reap/ACL restore"
        );
    } else {
        assert!(result.is_err());
        if cancel {
            assert_eq!(result, Err(launcher::LaunchError::Stopped));
            tokio::time::timeout(Duration::from_secs(5), async {
                while work
                    .store
                    .settled_usage(work.execution.attempt_id)
                    .await
                    .expect("retained accounting")
                    .is_none()
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("cancellation retains detached settlement");
        }
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
            usize::from(cancel || matches!(mode, Mode::MalformedUsage))
        );
        if matches!(mode, Mode::MalformedUsage) {
            assert_eq!(
                work.store
                    .attempt(work.execution.attempt_id)
                    .await
                    .expect("uncertain reservation")
                    .state,
                AttemptState::Uncertain
            );
        }
        assert_eq!(
            retained.len(),
            1,
            "failed native operation retained for protected inspection, never described as rolled back"
        );
        assert!(retained[0].to_string_lossy().starts_with("pi-"));
    }
    provider.abort();
    db.cleanup().await.expect("dispose synthetic database");
}

fn record_failure_metadata(
    runtime: &std::path::Path,
    retained: &[&std::ffi::OsString],
    result: Result<[u8; 32], launcher::LaunchError>,
    fake: &Fake,
    cancel: bool,
) -> std::io::Result<()> {
    use std::io::Write;
    for name in retained {
        if !name.to_string_lossy().starts_with("pi-") {
            continue;
        }
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "phase": "launcher_returned",
            "launch_error": result.err().map(|error| format!("{error:?}")),
            "fake_count_calls": fake.count.load(Ordering::SeqCst),
            "fake_generate_calls": fake.generated.load(Ordering::SeqCst),
            "cancellation_requested": cancel,
        }))
        .expect("fixed diagnostic metadata");
        let path = runtime.join(name).join("rehearsal-launcher-result.json");
        let mut file = codifix_platform_files::windows::create_private_file(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    Ok(())
}
