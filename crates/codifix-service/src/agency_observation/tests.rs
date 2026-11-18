use super::*;
use codifix_config::agency_observation::AgencyExchangeProject;
use codifix_record::{AuthPrincipalKind, PermissionSet};
use std::collections::BTreeMap;
fn principal(id: Uuid, project: &str) -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        id,
        kind: AuthPrincipalKind::ServiceToken,
        display_name: "test".into(),
        email: None,
        issuer: None,
        subject: None,
        groups: vec![],
        global_role: None,
        global: PermissionSet::EMPTY,
        project_roles: BTreeMap::from([(
            project.into(),
            crate::auth::ProjectRoleGrant {
                role: codifix_record::AuthRole::Operator,
                permissions: PermissionSet::from_permissions(&[Permission::MemoryCurate]),
                source: "test".into(),
            },
        )]),
        credential_source: crate::auth::CredentialSource::ServiceToken,
        token_id: None,
        session_id: None,
        session_csrf_hash: None,
    }
}
#[test]
fn exact_allowlist_and_project_permission_are_both_required() {
    let id = Uuid::new_v4();
    let c = AgencyObservationConfig {
        enabled: true,
        area: "workshop".into(),
        instance_id: Uuid::new_v4(),
        peer_ids: vec!["homelab".into()],
        legacy_exchanges: vec![],
        exchange_projects: vec![AgencyExchangeProject {
            project: "codifix".into(),
            principal_ids: vec![id],
        }],
    };
    let p = principal(id, "codifix");
    assert!(exchange_access(&c, &p, "codifix"));
    assert!(!exchange_access(&c, &p, "other"));
    assert!(!exchange_access(
        &c,
        &principal(Uuid::new_v4(), "codifix"),
        "codifix"
    ));
    assert!(!exchange_access(&c, &principal(id, "other"), "codifix"));
}
#[test]
fn cursor_and_terminal_controls_are_bounded() {
    assert!(cursor(Some("bad")).is_err());
    assert!(cursor(Some(&"x".repeat(129))).is_err());
    let at = Utc::now();
    let id = Uuid::new_v4();
    assert_eq!(
        cursor(Some(&format!("{}|{id}", at.to_rfc3339()))).unwrap(),
        (at, id)
    );
    assert_eq!(safe("a\x1b[31m\rb\n", 6), "a[31mb");
}

async fn task(
    pool: &PgPool,
    instance: Uuid,
    project: Uuid,
    requester: &str,
    capability: &str,
    peer: Option<&str>,
) -> Uuid {
    use serde_json::json;
    let mandate = Uuid::new_v4();
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agency_mandates(id,project_id,subject_id,document) VALUES($1,$2,'test',$3)",
    )
    .bind(mandate)
    .bind(project)
    .bind(json!({"subject_instance_id":instance}))
    .execute(pool)
    .await
    .unwrap();
    let payload = if let Some(peer) = peer {
        json!({"capability":"delegate","resource":{"kind":"peer","id":peer},"payload":{"kind":"delegate","peer":peer,"request":{"objective":"Inspect peer status","capability":capability,"payload":{"kind":capability,"token":"DO_NOT_EXPOSE_RAW"}}}})
    } else {
        json!({"capability":capability,"payload":{"kind":capability,"token":"DO_NOT_EXPOSE_RAW"}})
    };
    let contract =
        json!({"specialist_id":"companion","objective":"Inspect peer status","status":"submitted"});
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO agency_tasks(id,project_id,mandate_id,requester,request_id,contract,payload) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(project).bind(mandate).bind(requester).bind(id.to_string()).bind(contract).bind(payload).execute(&mut *tx).await.unwrap();
    if !requester.starts_with("inbox-gateway:") && capability != "conversation_turn" {
        sqlx::query("INSERT INTO agency_task_observation_origins(task_id,home_instance_id,requester,requester_instance_id,requester_operator,peer,direction) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(id).bind(instance).bind(requester).bind(if peer.is_some(){instance}else{Uuid::new_v4()}).bind(peer.is_some()).bind(peer.unwrap_or(requester)).bind(if peer.is_some(){"outgoing"}else{"incoming"}).execute(&mut *tx).await.unwrap();
    }
    if let Some(peer) = peer {
        sqlx::query("INSERT INTO agency_remote_tasks(task_id,peer_id,request,remote_id) VALUES($1,$2,'{}',$3)").bind(id).bind(peer).bind(Uuid::new_v4().to_string()).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    id
}
#[tokio::test]
async fn database_projection_denies_wrong_scope_and_excludes_human_and_ambiguous_tasks() {
    use serde_json::json;
    use sqlx::ConnectOptions;
    use std::sync::{Arc, Mutex};
    let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let instance = Uuid::new_v4();
    let id = Uuid::new_v4();
    let project = Uuid::new_v4();
    let other = Uuid::new_v4();
    for (id, slug) in [(project, "codifix"), (other, "other")] {
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,$2,'/test')")
            .bind(id)
            .bind(slug)
            .execute(&db.pool)
            .await
            .unwrap();
    }
    let cfg=serde_json::from_value(json!({"service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-unused"},"database":{"url":db.pool.connect_options().to_url_lossy().as_str()},"offline":{"enabled":false},"cluster":{"enabled":false},"reinforcement":{"enabled":false},"consolidation":{"enabled":false},"background_work":{"enabled":false},"mcp":{"enabled":false},"embeddings":{"enabled":false,"create_enabled":false},"area":{"name":"workshop","instance_id":instance},"agency_observation":{"enabled":true,"area":"workshop","instance_id":instance,"peer_ids":["homelab"],"exchange_projects":[{"project":"codifix","principal_ids":[id]}]}})).unwrap();
    let mut state = crate::runtime::build_state(cfg, Arc::new(Mutex::new(None)))
        .await
        .unwrap();
    let p = principal(id, "codifix");
    // Explicit config alone cannot establish database area provenance.
    assert!(authorized(&state, &p, "codifix").await.is_err());
    sqlx::query(
        "INSERT INTO agency_runtime_heartbeats(instance_id,version) VALUES($1,'synthetic')",
    )
    .bind(instance)
    .execute(&db.pool)
    .await
    .unwrap();
    assert!(authorized(&state, &p, "codifix").await.is_ok());
    assert_eq!(
        authorized(&state, &principal(Uuid::new_v4(), "codifix"), "codifix")
            .await
            .unwrap_err()
            .status,
        axum::http::StatusCode::FORBIDDEN
    );
    assert!(authorized(&state, &p, "other").await.is_err());
    state.config.area.as_mut().unwrap().name = "homelab".into();
    assert!(authorized(&state, &p, "codifix").await.is_err());
    state.config.area.as_mut().unwrap().name = "workshop".into();
    let outgoing = task(
        &db.pool,
        instance,
        project,
        "operator",
        "inspect",
        Some("homelab"),
    )
    .await;
    let incoming = task(&db.pool, instance, project, "homelab", "inspect", None).await;
    let human = task(
        &db.pool,
        instance,
        project,
        "inbox-gateway:person",
        "inspect",
        Some("homelab"),
    )
    .await;
    let forwarded = task(
        &db.pool,
        instance,
        project,
        "operator",
        "conversation_turn",
        Some("homelab"),
    )
    .await;
    let ambiguous = task(
        &db.pool,
        instance,
        project,
        "operator",
        "inspect",
        Some("homelab"),
    )
    .await;
    sqlx::query("UPDATE agency_remote_tasks SET peer_id='unbound' WHERE task_id=$1")
        .bind(ambiguous)
        .execute(&db.pool)
        .await
        .unwrap();
    task(&db.pool, instance, other, "homelab", "inspect", None).await;
    let page = exchanges(
        State(state.clone()),
        Extension(p.clone()),
        Path("codifix".into()),
        Query(PageQuery::default()),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(page.exchanges.len(), 2);
    let first = exchanges(
        State(state.clone()),
        Extension(p.clone()),
        Path("codifix".into()),
        Query(PageQuery {
            limit: Some(1),
            ..Default::default()
        }),
    )
    .await
    .unwrap()
    .0;
    assert!(first.next_before.is_some());
    let second = exchanges(
        State(state.clone()),
        Extension(p.clone()),
        Path("codifix".into()),
        Query(PageQuery {
            limit: Some(1),
            before: first.next_before,
            ..Default::default()
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(second.exchanges.len(), 1);
    assert_ne!(first.exchanges[0].task_id, second.exchanges[0].task_id);
    assert!(page.exchanges.iter().any(|e| e.task_id == outgoing));
    assert!(page.exchanges.iter().any(|e| e.task_id == incoming));
    for excluded in [human, forwarded, ambiguous] {
        assert_eq!(
            detail(
                State(state.clone()),
                Extension(p.clone()),
                Path(("codifix".into(), excluded))
            )
            .await
            .unwrap_err()
            .status,
            axum::http::StatusCode::NOT_FOUND
        );
    }
    let d = detail(
        State(state.clone()),
        Extension(p.clone()),
        Path(("codifix".into(), outgoing)),
    )
    .await
    .unwrap()
    .0;
    assert!(
        !serde_json::to_string(&d)
            .unwrap()
            .contains("DO_NOT_EXPOSE_RAW")
    );
    assert!(d.exchange.journal_available);
    assert!(!d.independently_verified);
    // Legacy labels alone deny. Only an exact reviewed receipt permits projection.
    sqlx::query("DELETE FROM agency_task_observation_origins WHERE task_id=$1")
        .bind(outgoing)
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(
        detail(
            State(state.clone()),
            Extension(p.clone()),
            Path(("codifix".into(), outgoing))
        )
        .await
        .is_err()
    );
    sqlx::query("INSERT INTO area_import_receipts(id,target_instance,area,source_instance,snapshot_id,inventory_sha256,manifest_sha256,partition_sha256,source_migration,target_migration,row_count,projection_count) VALUES($1,$2,'workshop',$3,$4,repeat('a',64),repeat('b',64),repeat('c',64),56,65,0,0)")
        .bind(Uuid::new_v4()).bind(instance).bind(Uuid::new_v4()).bind(Uuid::new_v4()).execute(&db.pool).await.unwrap();
    let remote: Option<String> =
        sqlx::query_scalar("SELECT remote_id FROM agency_remote_tasks WHERE task_id=$1")
            .bind(outgoing)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    state
        .config
        .agency_observation
        .as_mut()
        .unwrap()
        .legacy_exchanges
        .push(codifix_config::agency_observation::LegacyExchangeReceipt {
            task_id: outgoing,
            peer: "homelab".into(),
            direction: "outgoing".into(),
            requester: "operator".into(),
            remote_task_id: remote,
        });
    // Legacy rows have no original journal; show only fields of the surviving snapshot.
    sqlx::query("DELETE FROM agency_observation_events WHERE task_id=$1")
        .bind(outgoing)
        .execute(&db.pool)
        .await
        .unwrap();
    let d = detail(
        State(state.clone()),
        Extension(p.clone()),
        Path(("codifix".into(), outgoing)),
    )
    .await
    .unwrap()
    .0;
    assert!(!d.exchange.journal_available);
    assert!(d.remote_task_id.is_some());
    let mut bad = state.clone();
    bad.config
        .agency_observation
        .as_mut()
        .unwrap()
        .legacy_exchanges[0]
        .remote_task_id = Some("unmatched".into());
    assert!(
        detail(
            State(bad),
            Extension(p.clone()),
            Path(("codifix".into(), outgoing))
        )
        .await
        .is_err()
    );
    assert!(d.events.iter().all(|e| e.kind == "snapshot"));
    // Heartbeat reports presence, does not imply peer reachability or recent work.
    let status = companion(
        State(state.clone()),
        Extension(p.clone()),
        Path("codifix".into()),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(status.state, "fresh");
    assert!(status.exchange_access);
    assert!(status.last_exchange_at.is_some());
    assert!(
        companion(
            State(state.clone()),
            Extension(principal(Uuid::new_v4(), "codifix")),
            Path("codifix".into())
        )
        .await
        .unwrap()
        .0
        .last_exchange_at
        .is_none()
    );
    sqlx::query(
        "UPDATE agency_runtime_heartbeats SET observed_at=clock_timestamp()-interval '5 minutes'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        companion(
            State(state.clone()),
            Extension(p.clone()),
            Path("codifix".into())
        )
        .await
        .unwrap()
        .0
        .state,
        "stale"
    );
    // Missing provenance schema is a hard read failure, never an empty success.
    sqlx::query("DROP TABLE inbox_task_origins")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(
        exchanges(
            State(state.clone()),
            Extension(p.clone()),
            Path("codifix".into()),
            Query(PageQuery::default())
        )
        .await
        .is_err()
    );
    state.config.agency_observation = None;
    assert_eq!(
        companion(State(state), Extension(p), Path("codifix".into()))
            .await
            .unwrap()
            .0
            .state,
        "disabled"
    );
    db.cleanup().await.unwrap();
}

#[tokio::test]
async fn journal_is_transactional_ordered_and_preserves_clarification_rounds() {
    use serde_json::json;
    let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let instance = Uuid::new_v4();
    let project = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,'journal','Journal','/test')",
    )
    .bind(project)
    .execute(&db.pool)
    .await
    .unwrap();
    let id = task(&db.pool, instance, project, "homelab", "inspect", None).await;
    for c in [
        json!({"status":"input_required","clarification":{"question":"Which scope?","answer":null}}),
        json!({"status":"working","clarification":{"question":"Which scope?","answer":"Reviewed scope"}}),
        json!({"status":"input_required","clarification":{"question":"Which revision?","answer":null}}),
        json!({"status":"completed","outcome":{"summary":"Peer reported inspected"}}),
    ] {
        sqlx::query("UPDATE agency_tasks SET contract=$2 WHERE id=$1")
            .bind(id)
            .bind(c)
            .execute(&db.pool)
            .await
            .unwrap();
    }
    let events:Vec<(i64,String,String)>=sqlx::query_as("SELECT sequence,kind,body FROM agency_observation_events WHERE task_id=$1 ORDER BY sequence").bind(id).fetch_all(&db.pool).await.unwrap();
    assert!(events.windows(2).all(|w| w[0].0 < w[1].0));
    assert_eq!(events.iter().filter(|e| e.1 == "clarification").count(), 2);
    assert!(
        events
            .iter()
            .any(|e| e.1 == "answer" && e.2 == "Reviewed scope")
    );
    assert!(
        events
            .iter()
            .any(|e| e.1 == "result" && e.2 == "Peer reported inspected")
    );
    let count = events.len();
    let mut tx = db.pool.begin().await.unwrap();
    sqlx::query("UPDATE agency_tasks SET contract=$2 WHERE id=$1")
        .bind(id)
        .bind(json!({"status":"failed"}))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM agency_observation_events WHERE task_id=$1"
        )
        .bind(id)
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        count as i64
    );
    let protected = task(
        &db.pool,
        instance,
        project,
        "homelab",
        "openbao_recover",
        None,
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM agency_observation_events WHERE task_id=$1"
        )
        .bind(protected)
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        0
    );
    assert!(
        sqlx::query("UPDATE agency_task_observation_origins SET peer='changed' WHERE task_id=$1")
            .bind(id)
            .execute(&db.pool)
            .await
            .is_err()
    );
    sqlx::query("DELETE FROM agency_tasks WHERE id=$1")
        .bind(id)
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM agency_observation_events WHERE task_id=$1"
        )
        .bind(id)
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        0
    );
    db.cleanup().await.unwrap();
}

#[test]
fn observation_routes_use_exact_project_policy() {
    let policies = crate::auth::build_policy_table();
    for (path, permission) in [
        (
            "/v1/projects/codifix/agency/companion",
            Permission::MemoryRead,
        ),
        (
            "/v1/projects/codifix/agency/exchanges",
            Permission::MemoryCurate,
        ),
        (
            "/v1/projects/codifix/agency/exchanges/00000000-0000-0000-0000-000000000001",
            Permission::MemoryCurate,
        ),
    ] {
        let p = policies.at(path).unwrap();
        assert_eq!(p.value.len(), 1);
        assert_eq!(p.value[0].0, axum::http::Method::GET);
        assert_eq!(p.value[0].1.permission, permission);
        assert_eq!(p.value[0].1.scope, crate::auth::ProjectScope::PathProject);
    }
}
