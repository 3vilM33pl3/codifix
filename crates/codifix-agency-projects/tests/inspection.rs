#![allow(clippy::unwrap_used)]
//! Synthetic provider responses; no live account or project state is accessed.
use axum::{Json, Router, extract::State, routing::post};
use codifix_agency_projects::{EntityKind, LinearClient};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;
type MockState = Arc<Mutex<(Value, Vec<Value>)>>;
async fn graphql(State(state): State<MockState>, Json(body): Json<Value>) -> Json<Value> {
    let mut s = state.lock().unwrap();
    s.1.push(body);
    Json(s.0.clone())
}
fn project(id: Uuid) -> Value {
    json!({"id":id,"updatedAt":"2026-10-31T12:00:00Z","name":"Observed project","description":"Observed description","content":"Observed content","lead":null,"teams":{"nodes":[{"id":Uuid::new_v4()}],"pageInfo":{"hasNextPage":false}},"url":"https://linear.example/project/observed","status":{"id":Uuid::new_v4(),"name":"In progress","type":"started"}})
}
#[tokio::test]
async fn inspection_queries_status_and_link_without_expanding_or_mutating_resources() {
    let id = Uuid::new_v4();
    let state = Arc::new(Mutex::new((json!({"data":{"object":project(id)}}), vec![])));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/graphql", listener.local_addr().unwrap());
    let router = Router::new()
        .route("/graphql", post(graphql))
        .with_state(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = LinearClient::new(
        &endpoint,
        "synthetic-readonly-token",
        Duration::from_secs(2),
    )
    .unwrap();
    let snapshot = client
        .inspect(EntityKind::Project, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.entity.id, id);
    assert_eq!(snapshot.entity.fields["name"], "Observed project");
    assert_eq!(
        snapshot.url.as_deref(),
        Some("https://linear.example/project/observed")
    );
    assert_eq!(snapshot.workflow_state.unwrap().category, "started");
    {
        let s = state.lock().unwrap();
        assert_eq!(s.1.len(), 1);
        assert!(
            s.1[0]["query"]
                .as_str()
                .unwrap()
                .starts_with("query AgencyInspect")
        );
        assert!(
            s.1[0]["query"]
                .as_str()
                .unwrap()
                .contains("status { id name type }")
        );
        assert_eq!(s.1[0]["variables"]["id"], id.to_string());
    }
    state.lock().unwrap().0 = json!({"data":{"object":null}});
    assert!(
        client
            .inspect(EntityKind::Project, id)
            .await
            .unwrap()
            .is_none()
    );
    server.abort();
}
#[tokio::test]
async fn inspection_rejects_identity_status_link_and_graphql_errors() {
    let id = Uuid::new_v4();
    let state = Arc::new(Mutex::new((json!({"data":{"object":project(id)}}), vec![])));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/graphql", listener.local_addr().unwrap());
    let router = Router::new()
        .route("/graphql", post(graphql))
        .with_state(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = LinearClient::new(
        &endpoint,
        "synthetic-readonly-token",
        Duration::from_secs(2),
    )
    .unwrap();
    let mut variants = vec![];
    let mut x = project(id);
    x["id"] = json!(Uuid::new_v4());
    variants.push(json!({"data":{"object":x}}));
    let mut x = project(id);
    x["status"] = Value::Null;
    variants.push(json!({"data":{"object":x}}));
    let mut x = project(id);
    x["url"] = json!("javascript:alert(1)");
    variants.push(json!({"data":{"object":x}}));
    let mut x = project(id);
    x["status"]["id"] = json!(Uuid::nil());
    variants.push(json!({"data":{"object":x}}));
    variants.push(json!({"data":{"object":project(id)},"errors":[{"message":"partial result is not success"}]}));
    for body in variants {
        state.lock().unwrap().0 = body;
        assert!(client.inspect(EntityKind::Project, id).await.is_err());
    }
    server.abort();
}
