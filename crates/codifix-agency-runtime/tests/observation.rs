#![allow(clippy::unwrap_used)]
//! Synthetic local database only: authenticated provenance and metadata heartbeat.
use chrono::{Duration, Utc};
use codifix_agency::*;
use codifix_agency_runtime::{
    config::{Config, Peer},
    store::{Store, Submit},
};
use serde_json::json;
use std::collections::BTreeSet;
use uuid::Uuid;

#[tokio::test]
async fn heartbeat_and_origins_preserve_the_authenticated_peer_boundary() {
    let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let home = Uuid::new_v4();
    let remote = Uuid::new_v4();
    let project = Uuid::new_v4();
    let mandate = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'observation','Observation','/synthetic')").bind(project).execute(&db.pool).await.unwrap();
    let store = Store {
        pool: db.pool.clone(),
    };
    let operator = Peer {
        id: "local-operator".into(),
        instance_id: home,
        token_env: "UNUSED_SYNTHETIC_OPERATOR".into(),
        operator: true,
        mandates: BTreeSet::new(),
        endpoint: None,
        outbound_token_env: None,
    };
    let peer = Peer {
        id: "homelab".into(),
        instance_id: remote,
        token_env: "UNUSED_SYNTHETIC_PEER".into(),
        operator: false,
        mandates: BTreeSet::from([mandate]),
        endpoint: None,
        outbound_token_env: None,
    };
    let mut cfg:Config=serde_json::from_value(json!({"instance_id":home,"listen":"127.0.0.1:0","public_url":"https://synthetic.invalid","database_env":"UNUSED_SYNTHETIC_DATABASE","specialists":[{"schema_version":1,"instance_id":home,"specialist_id":"inspector","name":"Inspector","capabilities":["inspect"]}],"peers":[operator,peer]})).unwrap();
    store.heartbeat(&cfg).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM agency_runtime_heartbeats")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    cfg.observation_enabled = true;
    store.heartbeat(&cfg).await.unwrap();
    let metadata:(Vec<String>,Vec<String>)=sqlx::query_as("SELECT enabled_specialists,enrolled_peers FROM agency_runtime_heartbeats WHERE instance_id=$1").bind(home).fetch_one(&db.pool).await.unwrap();
    assert_eq!(
        metadata,
        (vec!["inspector".to_string()], vec!["homelab".to_string()])
    );
    let scope = ResourceScope {
        kind: "instance".into(),
        id: home.to_string(),
    };
    let peer_scope = ResourceScope {
        kind: "peer".into(),
        id: "homelab".into(),
    };
    let grant = Mandate {
        id: mandate,
        parent_id: None,
        issuer_instance_id: home,
        subject_instance_id: home,
        specialist_id: "inspector".into(),
        capabilities: BTreeSet::from(["inspect".into(), "delegate".into()]),
        resources: BTreeSet::from([scope.clone(), peer_scope.clone()]),
        destinations: BTreeSet::new(),
        expires_at: Utc::now() + Duration::hours(1),
        status: MandateStatus::Active,
        budget: Budget {
            micro_usd: 0,
            calls: 100,
            turns: 100,
            delegation_depth: 1,
        },
    };
    store
        .install_mandate(project, &grant, &operator, home)
        .await
        .unwrap();
    let mut request = Submit {
        request_id: Uuid::new_v4().to_string(),
        mandate_id: mandate,
        specialist_id: "inspector".into(),
        objective: "Inspect status".into(),
        acceptance_criteria: vec!["status_observed".into()],
        capability: "inspect".into(),
        resource: scope,
        deadline: Utc::now() + Duration::minutes(5),
        budget: Budget {
            micro_usd: 0,
            calls: 1,
            turns: 2,
            delegation_depth: 0,
        },
        payload: json!({"kind":"inspect"}),
        parent_id: None,
    };
    let incoming = store.submit_observed(&request, &peer, true).await.unwrap();
    let origin:(Uuid,Uuid,String,bool)=sqlx::query_as("SELECT home_instance_id,requester_instance_id,direction,requester_operator FROM agency_task_observation_origins WHERE task_id=$1").bind(incoming.contract.id).fetch_one(&db.pool).await.unwrap();
    assert_eq!(origin, (home, remote, "incoming".into(), false));
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT kind FROM agency_observation_events WHERE task_id=$1"
        )
        .bind(incoming.contract.id)
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        "request"
    );
    request.request_id = Uuid::new_v4().to_string();
    let local = store.submit(&request, &operator).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM agency_task_observation_origins WHERE task_id=$1"
        )
        .bind(local.contract.id)
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        0
    );
    request.request_id = Uuid::new_v4().to_string();
    request.capability = "delegate".into();
    request.resource = peer_scope;
    request.payload = json!({"kind":"delegate","peer":"homelab","request":{"capability":"inspect","payload":{"kind":"inspect"}}});
    let outgoing = store
        .submit_observed(&request, &operator, true)
        .await
        .unwrap();
    let origin: (String, String) = sqlx::query_as(
        "SELECT direction,peer FROM agency_task_observation_origins WHERE task_id=$1",
    )
    .bind(outgoing.contract.id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(origin, ("outgoing".into(), "homelab".into()));
    for protected in ["openbao_recover", "delegate"] {
        request.request_id = Uuid::new_v4().to_string();
        request.payload = json!({"kind":"delegate","peer":"homelab","request":{"capability":protected,"payload":{"kind":protected}}});
        let task = store
            .submit_observed(&request, &operator, true)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM agency_task_observation_origins WHERE task_id=$1"
            )
            .bind(task.contract.id)
            .fetch_one(&db.pool)
            .await
            .unwrap(),
            0
        );
    }
    request.request_id = Uuid::new_v4().to_string();
    request.capability = "inspect".into();
    request.resource = ResourceScope {
        kind: "instance".into(),
        id: home.to_string(),
    };
    request.payload = json!({"kind":"inspect"});
    let mut oversized = peer.clone();
    oversized.id = "x".repeat(129);
    let task = store
        .submit_observed(&request, &oversized, true)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM agency_task_observation_origins WHERE task_id=$1"
        )
        .bind(task.contract.id)
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        0
    );
    // Existing non-owner role: no observation grants and no private Inbox reads.
    let role = format!("observation_existing_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE ROLE {role} NOLOGIN"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query(&format!("GRANT USAGE ON SCHEMA public TO {role}"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        "GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA public TO {role}"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        "GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO {role}"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(&format!("REVOKE ALL ON agency_runtime_heartbeats,agency_task_observation_origins,agency_observation_events,inbox_task_origins,inbox_dispatches FROM {role}")).execute(&db.pool).await.unwrap();
    sqlx::query(&format!(
        "REVOKE ALL ON SEQUENCE agency_observation_events_sequence_seq FROM {role}"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    let limited_role = role.clone();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |connection, _| {
            let statement = format!("SET ROLE {limited_role}");
            Box::pin(async move {
                sqlx::query(&statement).execute(connection).await?;
                Ok(())
            })
        })
        .connect_with(db.pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let limited = Store { pool };
    request.request_id = Uuid::new_v4().to_string();
    request.capability = "inspect".into();
    request.resource = ResourceScope {
        kind: "instance".into(),
        id: home.to_string(),
    };
    request.payload = json!({"kind":"inspect"});
    let ordinary = limited.submit(&request, &peer).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM agency_task_observation_origins WHERE task_id=$1"
        )
        .bind(ordinary.contract.id)
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        0
    );
    let mut ordinary = ordinary;
    ordinary
        .contract
        .transition(TaskStatus::Working, Utc::now())
        .unwrap();
    limited.update_contract(&ordinary).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM agency_observation_events WHERE task_id=$1"
        )
        .bind(ordinary.contract.id)
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        0
    );
    // Explicitly enabled role needs only the observation grants, not Inbox access.
    sqlx::query(&format!(
        "GRANT SELECT,INSERT ON agency_task_observation_origins TO {role}"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        "GRANT INSERT ON agency_observation_events TO {role}"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        "GRANT USAGE ON SEQUENCE agency_observation_events_sequence_seq TO {role}"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    request.request_id = Uuid::new_v4().to_string();
    let captured = limited
        .submit_observed(&request, &peer, true)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM agency_observation_events WHERE task_id=$1"
        )
        .bind(captured.contract.id)
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    // A restarted worker records further state for already admitted observed work.
    let restarted = Store {
        pool: limited.pool.clone(),
    };
    let mut captured = captured;
    captured
        .contract
        .transition(TaskStatus::Working, Utc::now())
        .unwrap();
    restarted.update_contract(&captured).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM agency_observation_events WHERE task_id=$1"
        )
        .bind(captured.contract.id)
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        2
    );
    limited.pool.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&db.pool)
        .await
        .unwrap();
    db.cleanup().await.unwrap();
}
