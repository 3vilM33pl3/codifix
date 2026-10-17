// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use codifix_test_support::IsolatedDatabase;
use sqlx::ConnectOptions;
use std::sync::Arc;

pub(crate) struct Fixture {
    pub(crate) db: IsolatedDatabase,
    pub(crate) state: AppState,
    pub(crate) human: AuthenticatedPrincipal,
    admin: AuthenticatedPrincipal,
    pub(crate) project: Uuid,
    other: Uuid,
    pub(crate) membership: Uuid,
    session: String,
    csrf: String,
}
pub(crate) async fn fixture() -> Option<Fixture> {
    let db = IsolatedDatabase::new().await?;
    let instance = Uuid::new_v4();
    let config=serde_json::from_value(json!({"database":{"url":db.pool.connect_options().to_url_lossy().as_str()},"area":{"name":"workshop","instance_id":instance},
        "service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-owner"},"auth":{"mode":"single_user","public_base_url":"https://company.example.test","oidc":{"issuer_url":"https://auth.example.test/application/o/memory"}},
        "offline":{"enabled":false},"cluster":{"enabled":false},"background_work":{"enabled":false},"automation":{"enabled":false},"embeddings":{"enabled":false,"create_enabled":false},"consolidation":{"enabled":false},"reinforcement":{"enabled":false},
        "llm":{"provider":"ollama","model":"disabled","base_url":"http://127.0.0.1:1"}})).unwrap();
    let mut state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
        .await
        .unwrap();
    let owner = super::super::resolve_principal(
        &state,
        super::super::Credential::ApiToken("synthetic-owner".into()),
    )
    .await
    .unwrap()
    .unwrap();
    let project = Uuid::new_v4();
    let other = Uuid::new_v4();
    for (id, slug) in [(project, "local"), (other, "other")] {
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,$2,'/synthetic')")
            .bind(id)
            .bind(slug)
            .execute(&db.pool)
            .await
            .unwrap();
        let _ = super::super::onboarding::review_project(
            State(state.clone()),
            Extension(owner.clone()),
            axum::extract::Path(id),
            Json(OnboardingProjectReviewRequest {
                expected_revision: 0,
                entry: OnboardingProjectEntry {
                    area: OnboardingArea::Workshop,
                    owner_instance_id: instance,
                    repository_url: format!("https://github.com/example/{slug}.git"),
                    default_branch: "main".into(),
                    offline_allowed: true,
                    tools: vec![],
                    destinations: vec![],
                },
            }),
        )
        .await
        .unwrap();
    }
    let person = Uuid::new_v4();
    let adminid = Uuid::new_v4();
    for (id, name) in [(person, "Human"), (adminid, "Admin")] {
        sqlx::query("INSERT INTO auth_principals(id,kind,issuer,subject,display_name) VALUES($1,'human_oidc','https://auth.example.test/application/o/memory',$2,$2)").bind(id).bind(name).execute(&db.pool).await.unwrap();
    }
    sqlx::query("UPDATE auth_principals SET global_permissions_json=$2 WHERE id=$1")
        .bind(adminid)
        .bind(json!(Permission::ALL))
        .execute(&db.pool)
        .await
        .unwrap();
    let grant = super::super::grant_membership(
        &db.pool,
        &owner,
        AuthMembershipGrantRequest {
            principal_id: person,
            project: "local".into(),
            role: AuthRole::Writer,
        },
    )
    .await
    .unwrap();
    super::super::grant_membership(
        &db.pool,
        &owner,
        AuthMembershipGrantRequest {
            principal_id: person,
            project: "other".into(),
            role: AuthRole::Reader,
        },
    )
    .await
    .unwrap();
    let (session, csrf) = super::super::mint_web_session(&state, &db.pool, person)
        .await
        .unwrap();
    let (adminsession, _) = super::super::mint_web_session(&state, &db.pool, adminid)
        .await
        .unwrap();
    state.config.auth.mode = AuthMode::MultiUser;
    let human = super::super::resolve_principal(
        &state,
        super::super::Credential::BrowserSession(session.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    let admin = super::super::resolve_principal(
        &state,
        super::super::Credential::BrowserSession(adminsession),
    )
    .await
    .unwrap()
    .unwrap();
    Some(Fixture {
        db,
        state,
        human,
        admin,
        project,
        other,
        membership: grant.id,
        session,
        csrf,
    })
}
fn fresh_request() -> (OnboardingDeviceStartRequest, String, String) {
    let credential = format!("mlt_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let poll = format!(
        "poll_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    (
        OnboardingDeviceStartRequest {
            request_id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            name: "Synthetic Mac".into(),
            platform: "macos".into(),
            credential_sha256: fingerprint(&super::super::hash_secret(&credential)),
            poll_sha256: fingerprint(&super::super::hash_secret(&poll)),
        },
        credential,
        poll,
    )
}
fn peer(n: u8) -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::from(([192, 0, 2, n], 4251)))
}
fn approval(code: &str, id: Uuid, role: AuthRole) -> OnboardingDeviceApproveRequest {
    OnboardingDeviceApproveRequest {
        user_code: code.into(),
        projects: vec![OnboardingDeviceSelection {
            project_id: id,
            role,
            revision: 1,
        }],
        confirmed: true,
    }
}
pub(crate) async fn enroll(
    f: &Fixture,
    n: u8,
    role: AuthRole,
) -> (
    OnboardingDeviceStartRequest,
    String,
    OnboardingDevicePollResponse,
) {
    let (request, credential, _) = fresh_request();
    let Json(started) = start(State(f.state.clone()), peer(n), Json(request.clone()))
        .await
        .unwrap();
    let Json(response) = approve(
        State(f.state.clone()),
        Extension(f.human.clone()),
        Json(approval(&started.user_code, f.project, role)),
    )
    .await
    .unwrap();
    (request, credential, response)
}

#[tokio::test]
async fn start_exact_replay_poll_proof_and_timing() {
    let Some(f) = fixture().await else { return };
    let (request, credential, proof) = fresh_request();
    let Json(first) = start(State(f.state.clone()), peer(11), Json(request.clone()))
        .await
        .unwrap();
    let Json(replay) = start(State(f.state.clone()), peer(11), Json(request.clone()))
        .await
        .unwrap();
    assert_eq!(first.user_code, replay.user_code);
    assert_eq!(first.expires_at, replay.expires_at);
    assert!(
        first
            .verification_uri_complete
            .starts_with("https://company.example.test/?device_code=")
    );
    let mut changed = request.clone();
    changed.name = "Changed".into();
    assert_eq!(
        start(State(f.state.clone()), peer(11), Json(changed))
            .await
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
    let wrong = OnboardingDevicePollRequest {
        request_id: request.request_id,
        poll_secret: "wrong-but-bounded-poll-secret-value".into(),
    };
    assert_eq!(
        poll(State(f.state.clone()), Json(wrong))
            .await
            .unwrap_err()
            .status,
        StatusCode::UNAUTHORIZED
    );
    let untouched: bool = sqlx::query_scalar(
        "SELECT last_poll_at IS NULL FROM onboarding_device_requests WHERE request_id=$1",
    )
    .bind(request.request_id)
    .fetch_one(&f.db.pool)
    .await
    .unwrap();
    assert!(untouched);
    let Json(pending) = poll(
        State(f.state.clone()),
        Json(OnboardingDevicePollRequest {
            request_id: request.request_id,
            poll_secret: proof.clone(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(pending.status, "pending");
    assert_eq!(
        poll(
            State(f.state.clone()),
            Json(OnboardingDevicePollRequest {
                request_id: request.request_id,
                poll_secret: proof
            })
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::TOO_MANY_REQUESTS
    );
    let stored: Vec<u8> = sqlx::query_scalar(
        "SELECT credential_hash FROM onboarding_device_requests WHERE request_id=$1",
    )
    .bind(request.request_id)
    .fetch_one(&f.db.pool)
    .await
    .unwrap();
    assert_eq!(stored, super::super::hash_secret(&credential));
    assert!(!serde_json::to_string(&first).unwrap().contains(&credential));
}
#[tokio::test]
async fn approval_caps_replay_revision_and_two_device_independence() {
    let Some(f) = fixture().await else { return };
    let (request, credential, _) = fresh_request();
    let Json(started) = start(State(f.state.clone()), peer(12), Json(request.clone()))
        .await
        .unwrap();
    let Json(reviewed) = review(
        State(f.state.clone()),
        Extension(f.human.clone()),
        Query(ReviewQuery {
            code: started.user_code.clone(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(reviewed.projects.len(), 2);
    assert_eq!(reviewed.credential_sha256, request.credential_sha256);
    assert_eq!(
        approve(
            State(f.state.clone()),
            Extension(f.human.clone()),
            Json(approval(&started.user_code, f.other, AuthRole::Writer))
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::FORBIDDEN
    );
    let mut stale = approval(&started.user_code, f.project, AuthRole::Writer);
    stale.projects[0].revision = 99;
    assert_eq!(
        approve(
            State(f.state.clone()),
            Extension(f.human.clone()),
            Json(stale)
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        approve(
            State(f.state.clone()),
            Extension(f.human.clone()),
            Json(approval(&started.user_code, f.project, AuthRole::Admin))
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::BAD_REQUEST
    );
    let selection = approval(&started.user_code, f.project, AuthRole::Writer);
    let Json(approved) = approve(
        State(f.state.clone()),
        Extension(f.human.clone()),
        Json(selection.clone()),
    )
    .await
    .unwrap();
    assert_eq!(approved.status, "approved");
    assert_eq!(approved.device_id, Some(request.device_id));
    let Json(replay) = approve(
        State(f.state.clone()),
        Extension(f.human.clone()),
        Json(selection),
    )
    .await
    .unwrap();
    assert_eq!(replay.principal_id, approved.principal_id);
    assert_eq!(
        approve(
            State(f.state.clone()),
            Extension(f.human.clone()),
            Json(approval(&started.user_code, f.other, AuthRole::Reader))
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::CONFLICT
    );
    let first = super::super::resolve_principal(
        &f.state,
        super::super::Credential::ApiToken(credential.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(first.has_for_project("local", Permission::ActivityCapture));
    assert!(!first.has_for_project("other", Permission::MemoryRead));
    assert!(!first.has_anywhere(Permission::AuthManage));
    assert!(!first.has_anywhere(Permission::SystemAdmin));
    let Json(catalog) = catalog(State(f.state.clone()), Extension(first.clone()))
        .await
        .unwrap();
    assert_eq!(catalog["projects"].as_array().unwrap().len(), 1);
    let (second_request, second_credential, _) = enroll(&f, 13, AuthRole::Reader).await;
    let second = super::super::resolve_principal(
        &f.state,
        super::super::Credential::ApiToken(second_credential.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!second.has_for_project("local", Permission::ActivityCapture));
    let _ = logout(State(f.state.clone()), Extension(first))
        .await
        .unwrap();
    assert!(
        super::super::resolve_principal(&f.state, super::super::Credential::ApiToken(credential))
            .await
            .is_err()
    );
    assert!(
        super::super::resolve_principal(
            &f.state,
            super::super::Credential::ApiToken(second_credential)
        )
        .await
        .is_ok()
    );
    let new_request = OnboardingDeviceStartRequest {
        request_id: Uuid::new_v4(),
        ..second_request
    };
    assert_eq!(
        start(State(f.state.clone()), peer(13), Json(new_request))
            .await
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
}
#[tokio::test]
async fn human_revocation_removes_token_and_locked_job_authority() {
    let Some(f) = fixture().await else { return };
    let (_, credential, response) = enroll(&f, 14, AuthRole::Writer).await;
    let principal = super::super::resolve_principal(
        &f.state,
        super::super::Credential::ApiToken(credential.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    let actor = codifix_store::jobs::JobActor::Credential {
        principal: principal.id,
        token: principal.token_id,
        session: None,
    };
    let mut tx = f.db.pool.begin().await.unwrap();
    let before = super::super::locked_project_permissions(&f.state, &mut tx, &actor, f.project)
        .await
        .unwrap();
    assert!(before.contains(Permission::ActivityCapture));
    tx.rollback().await.unwrap();
    super::super::revoke_membership(&f.db.pool, &f.admin, f.membership)
        .await
        .unwrap();
    let updated = super::super::resolve_principal(
        &f.state,
        super::super::Credential::ApiToken(credential.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!updated.has_anywhere(Permission::MemoryRead));
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        super::super::locked_project_permissions(&f.state, &mut tx, &actor, f.project)
            .await
            .unwrap()
            .is_empty()
    );
    tx.rollback().await.unwrap();
    let _ = super::super::onboarding::disable_user(
        State(f.state.clone()),
        Extension(f.admin.clone()),
        axum::extract::Path(f.human.id),
    )
    .await
    .unwrap();
    assert!(
        super::super::resolve_principal(&f.state, super::super::Credential::ApiToken(credential))
            .await
            .is_err()
    );
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        super::super::locked_project_permissions(&f.state, &mut tx, &actor, f.project)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert!(response.projects.unwrap()[0].role == AuthRole::Writer);
}
#[tokio::test]
async fn expired_and_audit_failed_requests_cannot_issue_or_revive() {
    let Some(f) = fixture().await else { return };
    let (request, _, proof) = fresh_request();
    sqlx::query("INSERT INTO onboarding_device_requests(request_id,device_id,name,platform,credential_hash,poll_hash,user_code,expires_at) VALUES($1,$2,$3,$4,$5,$6,'OLD1-CODE',now()-interval '1 minute')")
        .bind(request.request_id).bind(request.device_id).bind(&request.name).bind(&request.platform).bind(digest(&request.credential_sha256).unwrap()).bind(digest(&request.poll_sha256).unwrap()).execute(&f.db.pool).await.unwrap();
    let Json(expired) = poll(
        State(f.state.clone()),
        Json(OnboardingDevicePollRequest {
            request_id: request.request_id,
            poll_secret: proof,
        }),
    )
    .await
    .unwrap();
    assert_eq!(expired.status, "expired");
    assert_eq!(
        approve(
            State(f.state.clone()),
            Extension(f.human.clone()),
            Json(approval("OLD1-CODE", f.project, AuthRole::Reader))
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::GONE
    );
    let (fresh, _, _) = fresh_request();
    let Json(started) = start(State(f.state.clone()), peer(15), Json(fresh.clone()))
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION reject_device_decision_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='onboarding.device.decide' THEN RAISE EXCEPTION 'synthetic audit rejection'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_device_decision_audit BEFORE INSERT ON auth_audit_events FOR EACH ROW EXECUTE FUNCTION reject_device_decision_audit();").execute(&f.db.pool).await.unwrap();
    assert!(
        approve(
            State(f.state.clone()),
            Extension(f.human.clone()),
            Json(approval(&started.user_code, f.project, AuthRole::Reader))
        )
        .await
        .is_err()
    );
    let pending: String =
        sqlx::query_scalar("SELECT status FROM onboarding_device_requests WHERE request_id=$1")
            .bind(fresh.request_id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(pending, "pending");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM onboarding_devices")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
#[test]
fn peer_start_rate_is_bounded() {
    for _ in 0..10 {
        limit_start(IpAddr::from([192, 0, 2, 200])).unwrap();
    }
    assert_eq!(
        limit_start(IpAddr::from([192, 0, 2, 200]))
            .unwrap_err()
            .status,
        StatusCode::TOO_MANY_REQUESTS
    );
}
#[tokio::test]
async fn http_public_start_human_session_and_csrf_boundaries() {
    let Some(f) = fixture().await else { return };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = crate::routes::build_http_app(f.state.clone());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let client = reqwest::Client::new();
    let (request, _, _) = fresh_request();
    let response = client
        .post(format!("{base}/v1/onboarding/device/start"))
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let started: OnboardingDeviceStartResponse = response.json().await.unwrap();
    let endpoint = format!(
        "{base}/v1/onboarding/device/review?code={}",
        started.user_code
    );
    assert_eq!(
        client
            .get(&endpoint)
            .bearer_auth("synthetic-owner")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let cookie = format!("codifix_session={}; codifix_csrf={}", f.session, f.csrf);
    assert_eq!(
        client
            .get(endpoint)
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let endpoint = format!("{base}/v1/onboarding/device/approve");
    let selection = approval(&started.user_code, f.project, AuthRole::Reader);
    assert_eq!(
        client
            .post(&endpoint)
            .header("cookie", &cookie)
            .json(&selection)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(endpoint)
            .header("cookie", cookie)
            .header("origin", "https://company.example.test")
            .header("x-csrf-token", &f.csrf)
            .json(&selection)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    server.abort();
}

#[tokio::test]
async fn logout_replay_is_narrow_and_cancel_serializes_with_approval() {
    let Some(f) = fixture().await else { return };
    let (request, credential, response) = enroll(&f, 16, AuthRole::Reader).await;
    let principal = super::super::resolve_principal(
        &f.state,
        super::super::Credential::ApiToken(credential.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    let Json(receipt) = logout(State(f.state.clone()), Extension(principal))
        .await
        .unwrap();
    assert_eq!(receipt["device_id"], request.device_id.to_string());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = crate::routes::build_http_app(f.state.clone());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let client = reqwest::Client::new();
    let replay = client
        .post(format!("{base}/v1/onboarding/device/logout"))
        .bearer_auth(&credential)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    let replay: Value = replay.json().await.unwrap();
    assert_eq!(replay["device_id"], request.device_id.to_string());
    assert_eq!(
        client
            .get(format!("{base}/v1/onboarding/projects"))
            .bearer_auth(&credential)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(format!("{base}/v1/projects/local/memories"))
            .bearer_auth(&credential)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    server.abort();
    let (pending, _, proof) = fresh_request();
    let Json(started) = start(State(f.state.clone()), peer(17), Json(pending.clone()))
        .await
        .unwrap();
    let invalid = cancel(
        State(f.state.clone()),
        Json(OnboardingDevicePollRequest {
            request_id: pending.request_id,
            poll_secret: "invalid-poll-proof-before-cancellation".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(invalid.status, StatusCode::UNAUTHORIZED);
    let cancelled = cancel(
        State(f.state.clone()),
        Json(OnboardingDevicePollRequest {
            request_id: pending.request_id,
            poll_secret: proof.clone(),
        }),
    );
    let approved = approve(
        State(f.state.clone()),
        Extension(f.human.clone()),
        Json(approval(&started.user_code, f.project, AuthRole::Reader)),
    );
    let (cancelled, approved) = tokio::join!(cancelled, approved);
    assert_ne!(cancelled.is_ok(), approved.is_ok());
    if cancelled.is_ok() {
        assert_eq!(approved.unwrap_err().status, StatusCode::CONFLICT);
        let Json(replayed) = cancel(
            State(f.state.clone()),
            Json(OnboardingDevicePollRequest {
                request_id: pending.request_id,
                poll_secret: proof,
            }),
        )
        .await
        .unwrap();
        assert_eq!(replayed["device_id"], pending.device_id.to_string());
    } else {
        assert_eq!(cancelled.unwrap_err().status, StatusCode::CONFLICT);
    }
    assert!(response.projects.is_some());
}

#[tokio::test]
async fn linked_human_lock_precedes_device_and_token_locks() {
    let Some(f) = fixture().await else { return };
    let (_, credential, _) = enroll(&f, 18, AuthRole::Writer).await;
    let principal =
        super::super::resolve_principal(&f.state, super::super::Credential::ApiToken(credential))
            .await
            .unwrap()
            .unwrap();
    let actor = codifix_store::jobs::JobActor::Credential {
        principal: principal.id,
        token: principal.token_id,
        session: None,
    };
    let mut disabling = f.db.pool.begin().await.unwrap();
    sqlx::query("UPDATE auth_principals SET disabled_at=now() WHERE id=$1")
        .bind(f.human.id)
        .execute(&mut *disabling)
        .await
        .unwrap();
    let pool = f.db.pool.clone();
    let state = f.state.clone();
    let project = f.project;
    let mut pending = tokio::spawn(async move {
        let mut tx = pool.begin().await.unwrap();
        let result =
            super::super::locked_project_permissions(&state, &mut tx, &actor, project).await;
        tx.rollback().await.unwrap();
        result
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut pending)
            .await
            .is_err()
    );
    // A helper waiting on the human must not already hold either of these
    // rows, otherwise the normal human->device->token disable order deadlocks.
    sqlx::query("SELECT id FROM onboarding_devices WHERE service_token_id=$1 FOR UPDATE NOWAIT")
        .bind(principal.token_id)
        .fetch_one(&mut *disabling)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM auth_service_tokens WHERE id=$1 FOR UPDATE NOWAIT")
        .bind(principal.token_id)
        .fetch_one(&mut *disabling)
        .await
        .unwrap();
    disabling.commit().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
}
