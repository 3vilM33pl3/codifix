// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

#[tokio::test]
async fn collection_http_requires_exact_review_and_queues_metadata_only() {
    let Some(f) = Fixture::with_expertise(true).await else {
        return;
    };
    f.grant(f.writer_id, true).await;
    let fence = f
        .gateway
        .initialize(f.runtime.space, f.runtime.authority)
        .await
        .unwrap();
    let mut membership = intent(
        f.runtime.space,
        f.runtime.authority,
        PublicationRecord::Membership {
            member: f.runtime.instance,
            capability: SpaceCapability::AnswerExpertise,
        },
    );
    membership.envelope.schema_version = 3;
    assert_eq!(
        f.gateway
            .accept(fence, f.runtime.authority, &membership)
            .await
            .unwrap()
            .outcome,
        PublicationOutcome::Applied
    );
    f.publish_fixture_receipts().await;
    f.runtime.sync.catch_up().await.unwrap();
    assert!(f.request(&f.root, reqwest::Method::POST, "/grants", json!({"principal":f.writer_id,"enabled":true,"capabilities":["pull","publish","answer_expertise"]})).await.0.is_success());
    let canonical = f.source().await;
    let session = format!("session-{}", Uuid::new_v4());
    let csrf = format!("csrf-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,expires_at) VALUES($1,$2,$3,$4,now()+interval '1 hour')")
        .bind(Uuid::new_v4()).bind(f.writer_id).bind(crate::auth::hash_secret(&session)).bind(crate::auth::hash_secret(&csrf)).execute(&f.pool).await.unwrap();
    let client = reqwest::Client::new();
    let cookie = format!("codifix_session={session}; codifix_csrf={csrf}");
    let post = |path: &str, body: Value| {
        client
            .post(f.path(path))
            .header("cookie", &cookie)
            .header("origin", "https://local.example.test")
            .header("x-csrf-token", &csrf)
            .json(&body)
            .send()
    };
    let blueprint = json!({"collection":Uuid::new_v4(),"name":"Incident expertise",
        "description":"Reviewed incident knowledge","topics":["incident"],
        "modes":["deterministic"],"requesters":[f.runtime.instance],
        "model_destinations":[],"output_destinations":["local_human"],
        "entries":[{"canonical":canonical,"evidence":[]}]});
    assert_eq!(
        f.request(
            &f.writer,
            reqwest::Method::POST,
            "/collections/draft",
            blueprint.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(f.path("/collections/draft"))
            .header("cookie", &cookie)
            .json(&blueprint)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = post("/collections/draft", blueprint.clone()).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    assert_eq!(response.headers()["cache-control"], "no-store");
    let draft: Value = response.json().await.unwrap();
    assert!(!draft.to_string().contains("NEVER_SHARE"));
    assert_eq!(
        draft["snapshot"]["entries"][0]["content"]["text"],
        "Reviewed incident diagnosis"
    );
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM federation_expertise_reviews WHERE project_id=$1")
            .bind(f.runtime.project)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(n, 0, "preview must not activate standing consent");
    let activate = json!({"draft":draft,"confirmed_digest":draft["review_digest"]});
    let mut altered = activate.clone();
    altered["draft"]["advertisement"]["name"] = json!("Unreviewed rename");
    assert!(
        !post("/collections/activate", altered)
            .await
            .unwrap()
            .status()
            .is_success()
    );
    let mut egress = blueprint.clone();
    egress["output_destinations"] = json!(["cloud_model"]);
    assert_eq!(
        post("/collections/draft", egress).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    for _ in 0..2 {
        let response = post("/collections/activate", activate.clone())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::ACCEPTED,
            "{}",
            response.text().await.unwrap()
        );
        assert_eq!(
            response.json::<Value>().await.unwrap()["submission"],
            "queued"
        );
    }
    let rows: Vec<sqlx::types::Json<PublicationIntent>> =
        sqlx::query_scalar("SELECT intent FROM federation_expertise_reviews WHERE project_id=$1")
            .bind(f.runtime.project)
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1);
    let wire = serde_json::to_string(&rows[0].0).unwrap();
    assert!(!wire.contains("Reviewed incident diagnosis"));
    assert!(!wire.contains("NEVER_SHARE"));
    // Withdrawal disables local standing consent even before remote acceptance.
    let withdrawal = json!({"request_id":Uuid::new_v4(),"collection":blueprint["collection"],"expected_revision":1,"confirmed":true});
    for _ in 0..2 {
        let response = post("/collections/withdraw", withdrawal.clone())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::ACCEPTED,
            "{}",
            response.text().await.unwrap()
        );
    }
    let active:bool=sqlx::query_scalar("SELECT r.active FROM federation_expertise_collections c JOIN federation_expertise_reviews r USING(space,owner,project_id,collection,revision) WHERE c.project_id=$1").bind(f.runtime.project).fetch_one(&f.pool).await.unwrap();
    assert!(!active);
    let mut readonly = f.state.clone();
    readonly.config.service.read_only = true;
    let (readonly_url, task) = serve(crate::routes::build_http_app(readonly)).await;
    assert_eq!(
        client
            .post(
                f.path("/collections/activate")
                    .replace(&f.url, &readonly_url)
            )
            .header("cookie", &cookie)
            .header("origin", "https://local.example.test")
            .header("x-csrf-token", &csrf)
            .json(&activate)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    task.abort();
    let response = post("/collections/draft", blueprint.clone()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let stale: Value = response.json().await.unwrap();
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,search_document,created_at,updated_at) VALUES($1,$2,$3,28,FALSE,'Changed diagnosis','Incident','implementation','project',3,0.9,to_tsvector('english','incident'),now(),now())")
        .bind(Uuid::new_v4()).bind(f.runtime.project).bind(canonical.as_uuid()).execute(&f.pool).await.unwrap();
    assert!(
        !post(
            "/collections/activate",
            json!({"draft":stale,"confirmed_digest":stale["review_digest"]})
        )
        .await
        .unwrap()
        .status()
        .is_success()
    );
    let active:bool=sqlx::query_scalar("SELECT r.active FROM federation_expertise_collections c JOIN federation_expertise_reviews r USING(space,owner,project_id,collection,revision) WHERE c.project_id=$1").bind(f.runtime.project).fetch_one(&f.pool).await.unwrap();
    assert!(!active, "stale activation must leave withdrawal intact");
    let get = |path: &str| client.get(f.path(path)).header("cookie", &cookie).send();
    let detail_path = format!("/collections/{}", blueprint["collection"].as_str().unwrap());
    let detail = get(&detail_path).await.unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    assert_eq!(detail.headers()["cache-control"], "no-store");
    let detail: Value = detail.json().await.unwrap();
    assert_eq!(detail["advertisement"]["active"], false);
    assert_eq!(
        detail["snapshot"]["entries"][0]["content"]["text"], "Reviewed incident diagnosis",
        "inspection must not silently substitute the new source version"
    );
    assert!(!detail.to_string().contains("NEVER_SHARE"));
    assert_eq!(
        get(&format!("/collections/{}", Uuid::new_v4()))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get("/collections?limit=0").await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        get("/collections?after=not-a-uuid").await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    let mut second = blueprint.clone();
    second["collection"] = json!(Uuid::new_v4());
    let review: Value = post("/collections/draft", second)
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        post(
            "/collections/activate",
            json!({"draft":review,"confirmed_digest":review["review_digest"]})
        )
        .await
        .unwrap()
        .status(),
        StatusCode::ACCEPTED
    );
    let page = get("/collections?limit=1").await.unwrap();
    assert_eq!(page.headers()["cache-control"], "no-store");
    let page: Value = page.json().await.unwrap();
    assert_eq!(page["collections"].as_array().unwrap().len(), 1);
    assert!(
        !page.to_string().contains("diagnosis"),
        "listing must contain metadata only"
    );
    let cursor = page["next_after"].as_str().unwrap();
    let next: Value = get(&format!("/collections?limit=1&after={cursor}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(next["collections"].as_array().unwrap().len(), 1);
    assert_ne!(
        page["collections"][0]["collection"],
        next["collections"][0]["collection"]
    );
    assert!(next["next_after"].is_null());
    assert_eq!(
        f.request(&f.writer, reqwest::Method::GET, &detail_path, json!(null))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    // A collection grant is separate from publication and answering authority.
    let grant_path = format!("{detail_path}/grants");
    assert_eq!(
        post(
            &grant_path,
            json!({"principal":f.writer_id,"enabled":true,"confirmed":true})
        )
        .await
        .unwrap()
        .status(),
        StatusCode::FORBIDDEN
    );
    f.grant(f.root_id, true).await;
    let admin_session = format!("admin-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,expires_at) VALUES($1,$2,$3,$4,now()+interval '1 hour')")
        .bind(Uuid::new_v4()).bind(f.root_id).bind(crate::auth::hash_secret(&admin_session)).bind(crate::auth::hash_secret(&csrf)).execute(&f.pool).await.unwrap();
    let admin_cookie = format!("codifix_session={admin_session}; codifix_csrf={csrf}");
    let grant = |target: Uuid, enabled: bool, confirmed: bool| {
        client
            .post(f.path(&grant_path))
            .header("cookie", &admin_cookie)
            .header("origin", "https://local.example.test")
            .header("x-csrf-token", &csrf)
            .json(&json!({"principal":target,"enabled":enabled,"confirmed":confirmed}))
            .send()
    };
    // Existing root is not a project-scoped service identity.
    assert_eq!(
        grant(f.root_id, true, true).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let service = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'internal','synthetic answerer')").bind(service).execute(&f.pool).await.unwrap();
    assert_eq!(
        grant(service, true, true).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'operator')")
        .bind(Uuid::new_v4()).bind(service).bind(f.runtime.project).execute(&f.pool).await.unwrap();
    assert_eq!(
        grant(service, true, false).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        grant(service, true, true).await.unwrap().status(),
        StatusCode::OK
    );
    let enabled:bool=sqlx::query_scalar("SELECT enabled FROM federation_expertise_collection_grants WHERE space=$1 AND collection=$2 AND principal_id=$3")
        .bind(f.runtime.space.as_uuid()).bind(blueprint["collection"].as_str().unwrap().parse::<Uuid>().unwrap()).bind(service).fetch_one(&f.pool).await.unwrap();
    assert!(enabled);
    sqlx::query("UPDATE auth_principals SET disabled_at=clock_timestamp() WHERE id=$1")
        .bind(service)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        grant(service, true, true).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        grant(service, false, true).await.unwrap().status(),
        StatusCode::OK
    );
    let enabled:bool=sqlx::query_scalar("SELECT enabled FROM federation_expertise_collection_grants WHERE space=$1 AND collection=$2 AND principal_id=$3")
        .bind(f.runtime.space.as_uuid()).bind(blueprint["collection"].as_str().unwrap().parse::<Uuid>().unwrap()).bind(service).fetch_one(&f.pool).await.unwrap();
    assert!(!enabled, "disabled identities must remain revocable");
    let inspected = client
        .get(f.path(&format!("{grant_path}?principal={service}")))
        .header("cookie", &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(inspected.status(), StatusCode::OK);
    assert_eq!(inspected.headers()["cache-control"], "no-store");
    assert_eq!(inspected.json::<Value>().await.unwrap()["enabled"], false);
    assert_eq!(
        get(&format!("{grant_path}?principal={service}"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    f.grant(f.writer_id, false).await;
    assert!(!get(&detail_path).await.unwrap().status().is_success());
    assert!(!get("/collections").await.unwrap().status().is_success());
    assert!(
        !post("/collections/activate", activate)
            .await
            .unwrap()
            .status()
            .is_success()
    );
}
