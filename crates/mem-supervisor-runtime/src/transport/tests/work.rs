use super::*;
use crate::{
    launcher::{RunControl, SandboxConfig},
    work::{PinnedWork, WorkManifest, WorkStep},
};
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;

async fn manifest(db: &IsolatedDatabase, fake: &Fake) -> (tempfile::TempDir, WorkManifest, String) {
    let grant = super::disclosure::grant(db, fake).await;
    let manifest = WorkManifest {
        schema_version: 1,
        action_id: Uuid::new_v4(),
        source_event_id: Uuid::new_v4(),
        attempt_id: Uuid::new_v4(),
        grant,
        question: "Review this synthetic note".into(),
        sandbox: SandboxConfig {
            pi: "/no-artifact".into(),
            driver: "/no-driver".into(),
            driver_sha256: "0".repeat(64),
            private_runtime: "/no-runtime".into(),
        },
    };
    let dir = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .expect("private directory");
    let hash = save(&dir, &manifest);
    (dir, manifest, hash)
}
fn save(dir: &tempfile::TempDir, manifest: &WorkManifest) -> String {
    let bytes = serde_json::to_vec(manifest).expect("manifest");
    let path = dir.path().join("work.json");
    std::fs::write(&path, &bytes).expect("synthetic manifest");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).expect("private");
    format!("{:x}", Sha256::digest(&bytes))
}

#[tokio::test]
async fn fixed_work_restart_and_competing_claims_never_reissue_running_or_verified_actions() {
    let Some((db, store, _, _, fake, client, provider)) = fixture(Mode::Good).await else {
        return;
    };
    let (dir, manifest, hash) = manifest(&db, &fake).await;
    let path = dir.path().join("work.json");
    let work = PinnedWork::load(&path, &hash, fake.instance_id, Area::Workshop).expect("pinned");
    for state in [
        ActionState::Proposed,
        ActionState::Authorized,
        ActionState::Queued,
    ] {
        assert!(
            matches!(work.step(db.pool.clone(),RunControl::default()).await.expect("advance"),WorkStep::State(s) if s==state)
        );
    }
    let (a, b) = tokio::join!(
        work.step(db.pool.clone(), RunControl::default()),
        work.step(db.pool.clone(), RunControl::default())
    );
    let mut claims = Vec::new();
    for step in [a, b] {
        if let Ok(WorkStep::Claimed(claim)) = step {
            claims.push(claim);
        }
    }
    assert_eq!(claims.len(), 1, "only one worker obtains a paid-work lease");
    let claim = claims.pop().expect("claim");
    let resumed = PinnedWork::load(&path, &hash, fake.instance_id, Area::Workshop).expect("reload");
    assert!(matches!(
        resumed
            .step(db.pool.clone(), RunControl::default())
            .await
            .expect("resume"),
        WorkStep::State(ActionState::Running)
    ));
    *fake.expected.write().expect("expected") = Arc::clone(&claim.request);
    client
        .execute_guarded_and_persist(
            &claim.store,
            &claim.lease,
            &claim.execution,
            &claim.request,
            claim.disclosure.as_ref(),
        )
        .await
        .expect("one fake result");
    assert!(matches!(
        resumed
            .step(db.pool.clone(), RunControl::default())
            .await
            .expect("completed resume"),
        WorkStep::State(ActionState::Verified)
    ));
    let proposal = store
        .private_proposal(
            Area::Workshop,
            manifest.grant.project_id,
            manifest.action_id,
        )
        .await
        .expect("saved");
    let digest = Sha256::digest(serde_json::to_vec(&proposal.proposal).expect("proposal")).into();
    claim
        .confirm(digest)
        .await
        .expect("confirmed exact child hash");
    assert!(claim.confirm([0; 32]).await.is_err());
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    let mut altered = manifest;
    altered.question.push_str(" changed");
    let hash = save(&dir, &altered);
    let changed = PinnedWork::load(&path, &hash, fake.instance_id, Area::Workshop)
        .expect("valid file, wrong durable identity");
    assert!(
        changed
            .step(db.pool.clone(), RunControl::default())
            .await
            .is_err()
    );
    provider.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn abandoned_work_becomes_uncertain_not_queued_and_stopped_work_never_starts() {
    let Some((db, _, _, _, fake, _, provider)) = fixture(Mode::Good).await else {
        return;
    };
    let (dir, manifest, hash) = manifest(&db, &fake).await;
    let path = dir.path().join("work.json");
    for area in [Area::Trust, Area::Homelab, Area::General] {
        assert!(PinnedWork::load(&path, &hash, fake.instance_id, area).is_err());
    }
    assert!(PinnedWork::load(&path, &hash, Uuid::new_v4(), Area::Workshop).is_err());
    let work = PinnedWork::load(&path, &hash, fake.instance_id, Area::Workshop).expect("pinned");
    let stop = RunControl::default();
    stop.stop();
    assert!(work.step(db.pool.clone(), stop).await.is_err());
    for _ in 0..4 {
        work.step(db.pool.clone(), RunControl::default())
            .await
            .expect("claim");
    }
    sqlx::query("UPDATE supervisor_actions SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(manifest.action_id).execute(&db.pool).await.expect("simulate crash lease expiry");
    for _ in 0..3 {
        assert_eq!(
            work.observe(db.pool.clone())
                .await
                .expect("credential-free recovery"),
            ActionState::Uncertain
        );
        assert!(matches!(
            work.step(db.pool.clone(), RunControl::default())
                .await
                .expect("recover"),
            WorkStep::State(ActionState::Uncertain)
        ));
    }
    assert_eq!(fake.count.load(Ordering::SeqCst), 0);
    assert_eq!(fake.generated.load(Ordering::SeqCst), 0);
    provider.abort();
    db.cleanup().await.expect("cleanup");
}
