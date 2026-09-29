// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

#[tokio::test]
async fn configured_projects_share_stream_identity_but_not_access_or_output_policy() {
    let Some(f) = Fixture::with_expertise(true).await else {
        return;
    };
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'other','/other')")
        .bind(other)
        .bind(other.to_string())
        .execute(&f.pool)
        .await
        .unwrap();
    let (principal, token) = actor(&f.pool, other, "operator").await;
    let client = reqwest::Client::new();
    let url = format!("{}/v1/projects/{other}/federation", f.url);
    assert_eq!(
        client
            .get(&url)
            .header("x-api-token", &token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let binding = serde_json::from_value(json!({
        "project": other, "principal": principal, "expertise_output_destinations": ["openai"]
    }))
    .unwrap();
    let mut configured = (*f.runtime).clone();
    configured.projects.insert(other, binding);
    let view = configured.for_project(other).unwrap();
    assert_eq!(view.instance, f.runtime.instance);
    assert_eq!(view.space, f.runtime.space);
    assert!(!view.expertise_browser);
    assert_eq!(view.expertise_output_destinations, vec!["openai"]);
    assert!(view.expertise_callers.is_empty());
    assert_eq!(
        configured
            .for_project(f.runtime.project)
            .unwrap()
            .expertise_output_destinations,
        vec!["local_human"]
    );
    assert!(configured.for_project(Uuid::new_v4()).is_none());
    *f.state.federation.write().unwrap() = Some(Arc::new(configured));
    let response = client
        .get(&url)
        .header("x-api-token", &token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let discovery: Value = response.json().await.unwrap();
    assert_eq!(discovery["instance"], json!(f.runtime.instance));
    assert_eq!(discovery["principal"], json!(principal));
    assert_eq!(
        client
            .get(&url)
            .header("x-api-token", &f.reader)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let replicas: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM federation_replicas WHERE space=$1 AND receiver=$2",
    )
    .bind(f.runtime.space.as_uuid())
    .bind(f.runtime.instance.as_uuid())
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(replicas, 1);
}
