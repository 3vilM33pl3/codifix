//! Explicit production-launcher rehearsal: patched Pi, synthetic sources,
//! fake provider and disposable PostgreSQL. No real key or provider calls.
use super::*;
use crate::launcher::{self, RunControl, SandboxConfig};
use std::{os::unix::fs::PermissionsExt, path::PathBuf};

#[tokio::test]
#[ignore = "requires explicit patched Pi/driver paths, pinned local image and disposable PostgreSQL"]
async fn production_launcher_delivers_revokes_accounts_and_cleans_up() {
    run(Mode::Good, false).await;
    run(Mode::RevokeDuringCount, false).await;
    run(Mode::MalformedUsage, false).await;
    run(Mode::Good, true).await;
}

async fn run(mode: Mode, cancel: bool) {
    assert_eq!(
        std::env::var("MEMORY_LAYER_TEST_REQUIRE_DB").as_deref(),
        Ok("1")
    );
    let pi = PathBuf::from(
        std::env::var_os("MEMORY_PI_PATCHED_ARTIFACT").expect("explicit pinned artifact"),
    );
    let driver =
        PathBuf::from(std::env::var_os("MEMORY_PI_SANDBOX_DRIVER").expect("fresh sandbox driver"));
    let driver_sha256 = format!(
        "{:x}",
        Sha256::digest(std::fs::read(&driver).expect("driver bytes"))
    );
    // Build directories may be group-writable. Stage a private deployment copy
    // rather than weakening the production artifact-path checks for the test.
    let artifacts = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in("/data/tmp")
        .expect("private artifacts");
    let private_driver = artifacts.path().join("driver");
    std::fs::copy(&driver, &private_driver).expect("stage driver");
    std::fs::set_permissions(&private_driver, std::fs::Permissions::from_mode(0o500))
        .expect("driver permissions");
    let driver = private_driver;
    let (db, store, lease, revision, fake, client, provider) =
        fixture(mode).await.expect("required database");
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
            .expect("current sources and authority"),
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
    let directory = tempfile::Builder::new()
        .prefix("memory-pi-launch-test-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in("/data/tmp")
        .expect("private runtime");
    let config = SandboxConfig {
        pi,
        driver,
        driver_sha256,
        private_runtime: directory.path().into(),
        windows_artifacts: None,
    };
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
        .expect("provider started");
        control.stop();
        pause.notify_one();
    }
    let result = launched.await.expect("launcher join");
    assert_eq!(fake.count.load(Ordering::SeqCst), 1);
    if matches!(mode, Mode::Good) && !cancel {
        let hash = result.expect("production launcher delivers");
        let proposal = work
            .store
            .private_proposal(
                Area::Workshop,
                work.execution.project_id,
                work.lease.action_id(),
            )
            .await
            .expect("saved");
        let digest: [u8; 32] =
            Sha256::digest(serde_json::to_vec(&proposal.proposal).expect("proposal")).into();
        assert_eq!(hash, digest);
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
                .expect("state")
                .state,
            ActionState::Verified
        );
        assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    } else {
        assert!(result.is_err());
        if cancel {
            assert_eq!(result, Err(launcher::LaunchError::Stopped));
            tokio::time::timeout(Duration::from_secs(5), async {
                while work
                    .store
                    .settled_usage(work.execution.attempt_id)
                    .await
                    .expect("usage")
                    .is_none()
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("cancellation preserves settlement");
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
                    .expect("retained")
                    .state,
                AttemptState::Uncertain
            );
        }
    }
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("runtime")
            .count(),
        0,
        "owned sandbox files reclaimed after verified removal"
    );
    provider.abort();
    db.cleanup().await.expect("cleanup");
}
