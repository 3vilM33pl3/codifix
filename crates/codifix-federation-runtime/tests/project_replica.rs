// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use codifix_federation::{publication::*, subscription::*};
use codifix_federation_runtime::*;
use codifix_federation_store::{ProjectEntry, ProjectEvent, ProjectPage, SpacePolicy};
use codifix_record::OnboardingProjectSnapshotRequest;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn identity() -> InstanceId {
    InstanceId::new(Uuid::new_v4()).unwrap()
}
fn binding() -> ProjectReplicaBinding {
    ProjectReplicaBinding {
        subscription: Uuid::new_v4(),
        device: Uuid::new_v4(),
        catalog_revision: 1,
        authority: identity(),
        scope: ProjectScope {
            company: Uuid::new_v4(),
            area: "workshop".into(),
            owner: identity(),
            project: Uuid::new_v4(),
            space: SpaceId::new(Uuid::new_v4()).unwrap(),
        },
    }
}
#[derive(Clone, Copy)]
enum Fault {
    None,
    Device,
    DeniedSecond,
    PolicySecond,
}
struct FakeOwner {
    binding: ProjectReplicaBinding,
    security: GatewaySecurity,
    head: u64,
    calls: Mutex<Vec<OnboardingProjectSnapshotRequest>>,
    fault: Mutex<Fault>,
}
impl FakeOwner {
    fn new(binding: ProjectReplicaBinding, head: u64) -> Self {
        Self {
            binding,
            head,
            security: GatewaySecurity::initial(&[39; 32], SpacePolicy::default()).unwrap(),
            calls: Mutex::new(vec![]),
            fault: Mutex::new(Fault::None),
        }
    }
}
impl ProjectDeviceTransport for FakeOwner {
    fn snapshot<'a>(
        &'a self,
        project: Uuid,
        request: &'a OnboardingProjectSnapshotRequest,
    ) -> ProjectDeviceFuture<'a> {
        Box::pin(async move {
            assert_eq!(project, self.binding.scope.project);
            let call = {
                let mut calls = self.calls.lock().unwrap();
                calls.push(request.clone());
                calls.len()
            };
            let fault = *self.fault.lock().unwrap();
            if matches!(fault, Fault::DeniedSecond) && call == 2 {
                return Err(RuntimeError::Unauthorized);
            }
            let through = request.through.unwrap_or(self.head);
            let next = through.min(request.after + u64::from(request.limit));
            let page = ProjectPage {
                request: ProjectPageRequest {
                    nonce: request.nonce,
                    device: if matches!(fault, Fault::Device) {
                        Uuid::new_v4()
                    } else {
                        self.binding.device
                    },
                    scope: self.binding.scope.clone(),
                    catalog_revision: self.binding.catalog_revision,
                    policy_revision: if matches!(fault, Fault::PolicySecond) && call == 2 {
                        2
                    } else {
                        1
                    },
                    binding_revision: 1,
                    after: request.after,
                    through: request.through,
                    limit: request.limit,
                },
                authority: self.binding.authority,
                through,
                next,
                entries: (request.after + 1..=next)
                    .map(|sequence| ProjectEntry {
                        sequence,
                        event: ProjectEvent::Review {
                            review: ProjectStreamReview {
                                stream: StreamId {
                                    origin: self.binding.scope.owner,
                                    canonical: RecordId::new(Uuid::from_u128(sequence as u128))
                                        .unwrap(),
                                },
                                review_id: Uuid::from_u128(sequence as u128),
                                records: vec![],
                                enabled: false,
                            },
                        },
                    })
                    .collect(),
            };
            self.security.sign_project_page(page)
        })
    }
}

#[tokio::test]
async fn bounded_snapshot_resumes_from_committed_cursor_and_keeps_original_scope() {
    let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let binding = binding();
    let owner = FakeOwner::new(binding.clone(), 35);
    assert!(
        ProjectReplica::open(db.pool.clone(), binding.clone(), owner.security.trust())
            .await
            .is_err()
    );
    let absent: i64 = sqlx::query_scalar("SELECT count(*) FROM onboarding_project_projections")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(absent, 0);
    let replica =
        ProjectReplica::initialize(db.pool.clone(), binding.clone(), owner.security.trust())
            .await
            .unwrap();
    assert!(replica.status().await.unwrap().observed_at.is_none());
    let first = replica.synchronize(&owner, 1).await.unwrap();
    assert_eq!(
        (first.cursor, first.through, first.caught_up),
        (32, 35, false)
    );
    drop(replica);
    // Restart reuses trusted subscription identity and the transactional cursor.
    let replica = ProjectReplica::initialize(db.pool.clone(), binding, owner.security.trust())
        .await
        .unwrap();
    let second = replica.synchronize(&owner, 2).await.unwrap();
    assert_eq!(
        (second.cursor, second.through, second.caught_up),
        (35, 35, true)
    );
    assert!(replica.status().await.unwrap().observed_at.is_some());
    let third = replica.synchronize(&owner, 2).await.unwrap();
    assert_eq!((third.cursor, third.pages), (35, 1));
    assert!(replica.search("absent", 8).await.unwrap().is_empty());
    assert_eq!(owner.calls.lock().unwrap()[1].after, 32);
    let local: i64 = sqlx::query_scalar("SELECT count(*) FROM memory_entries")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(local, 0);
    drop(replica);
    db.cleanup().await.unwrap();
}

struct WaitingOwner {
    started: tokio::sync::Notify,
}
impl ProjectDeviceTransport for WaitingOwner {
    fn snapshot<'a>(
        &'a self,
        _: Uuid,
        _: &'a OnboardingProjectSnapshotRequest,
    ) -> ProjectDeviceFuture<'a> {
        Box::pin(async move {
            self.started.notify_one();
            std::future::pending().await
        })
    }
}
#[tokio::test]
async fn shutdown_cancels_network_wait_without_page_or_cursor_effects() {
    let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let bound = binding();
    let security = GatewaySecurity::initial(&[39; 32], SpacePolicy::default()).unwrap();
    let replica = ProjectReplica::initialize(db.pool.clone(), bound, security.trust())
        .await
        .unwrap();
    let owner = WaitingOwner {
        started: tokio::sync::Notify::new(),
    };
    let wake = tokio::sync::Notify::new();
    let (stop, stopping) = tokio::sync::watch::channel(false);
    let (health, _) = tokio::sync::watch::channel(ProjectSyncHealth::Unknown);
    {
        let watching = replica.watch(&owner, stopping, &wake, health);
        tokio::pin!(watching);
        tokio::select! { _=&mut watching=>panic!("worker stopped before shutdown"),_=owner.started.notified()=>{} }
        stop.send_replace(true);
        tokio::time::timeout(std::time::Duration::from_secs(1), &mut watching)
            .await
            .unwrap();
    }
    assert_eq!(replica.status().await.unwrap().cursor, 0);
    assert!(replica.status().await.unwrap().observed_at.is_none());
    let pages: i64 = sqlx::query_scalar("SELECT count(*) FROM onboarding_project_pages")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(pages, 0);
    drop(replica);
    db.cleanup().await.unwrap();
}

#[tokio::test]
async fn verified_wrong_audience_and_changed_policy_do_not_commit_the_rejected_page() {
    let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let owner = FakeOwner::new(binding(), 35);
    let replica = ProjectReplica::initialize(
        db.pool.clone(),
        owner.binding.clone(),
        owner.security.trust(),
    )
    .await
    .unwrap();
    *owner.fault.lock().unwrap() = Fault::Device;
    assert!(matches!(
        replica.synchronize(&owner, 2).await,
        Err(RuntimeError::InvalidResponse)
    ));
    assert_eq!(replica.status().await.unwrap().cursor, 0);
    assert!(replica.status().await.unwrap().observed_at.is_none());
    owner.calls.lock().unwrap().clear();
    *owner.fault.lock().unwrap() = Fault::PolicySecond;
    assert!(matches!(
        replica.synchronize(&owner, 2).await,
        Err(RuntimeError::InvalidResponse)
    ));
    assert_eq!(replica.status().await.unwrap().cursor, 32);
    *owner.fault.lock().unwrap() = Fault::None;
    assert!(replica.synchronize(&owner, 2).await.unwrap().caught_up);
    drop(replica);
    db.cleanup().await.unwrap();
}

#[tokio::test]
async fn revocation_interrupts_delivery_and_empty_project_is_a_valid_snapshot() {
    let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let owner = FakeOwner::new(binding(), 35);
    let replica = ProjectReplica::initialize(
        db.pool.clone(),
        owner.binding.clone(),
        owner.security.trust(),
    )
    .await
    .unwrap();
    *owner.fault.lock().unwrap() = Fault::DeniedSecond;
    assert!(matches!(
        replica.synchronize(&owner, 2).await,
        Err(RuntimeError::Unauthorized)
    ));
    assert_eq!(replica.status().await.unwrap().cursor, 32);
    let empty = FakeOwner::new(binding(), 0);
    let second = ProjectReplica::initialize(
        db.pool.clone(),
        empty.binding.clone(),
        empty.security.trust(),
    )
    .await
    .unwrap();
    let progress = second.synchronize(&empty, 1).await.unwrap();
    assert_eq!(
        (
            progress.cursor,
            progress.through,
            progress.pages,
            progress.caught_up
        ),
        (0, 0, 1, true)
    );
    let mut trust = empty.binding.clone();
    trust.scope.area = "trust".into();
    assert!(
        ProjectReplica::initialize(db.pool.clone(), trust, empty.security.trust())
            .await
            .is_err()
    );
    drop(replica);
    drop(second);
    db.cleanup().await.unwrap();
}

#[tokio::test]
async fn device_http_transport_sends_only_bounded_cursor_and_suppresses_failure_bodies() {
    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode},
        routing::post,
    };
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server_calls = calls.clone();
    async fn handler(
        State(calls): State<Arc<Mutex<Vec<OnboardingProjectSnapshotRequest>>>>,
        headers: HeaderMap,
        Json(request): Json<OnboardingProjectSnapshotRequest>,
    ) -> (StatusCode, &'static str) {
        assert_eq!(
            headers["authorization"],
            format!("Bearer mlt_{}", "a".repeat(64))
        );
        calls.lock().unwrap().push(request);
        (
            StatusCode::FORBIDDEN,
            "private denied project text must not reach diagnostics",
        )
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/v1/onboarding/projects/{project}/snapshot", post(handler))
        .with_state(server_calls);
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let endpoint = Endpoint::new(&format!("http://{address}"), true).unwrap();
    let client =
        ScopedDeviceClient::new(endpoint.clone(), format!("mlt_{}", "a".repeat(64))).unwrap();
    assert!(ScopedDeviceClient::new(endpoint, "gateway-secret".into()).is_err());
    let request = OnboardingProjectSnapshotRequest {
        nonce: Uuid::new_v4(),
        after: 0,
        through: None,
        limit: 32,
    };
    let error = client.snapshot(Uuid::new_v4(), &request).await.unwrap_err();
    assert!(matches!(error, RuntimeError::Unauthorized));
    assert!(!error.to_string().contains("private denied"));
    assert_eq!(calls.lock().unwrap().as_slice(), &[request]);
    server.abort();
}
