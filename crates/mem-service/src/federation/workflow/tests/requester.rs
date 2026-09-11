// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_federation::expertise::*;

pub(super) async fn require_committed_consent(
    State(pool): State<PgPool>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if request.uri().path() == "/v1/expertise" {
        // A separate gateway connection must already see the committed receipt
        // when submission arrives, not merely a row in the caller transaction.
        let (parts, body) = request.into_parts();
        let bytes = axum::body::to_bytes(body, 32768).await.unwrap();
        let rpc: mem_federation_runtime::ExpertiseRpc = serde_json::from_slice(&bytes).unwrap();
        if let mem_federation_runtime::ExpertiseOperation::Submit { question: q } = rpc.operation {
            let n:i64=sqlx::query_scalar("SELECT count(*) FROM federation_expertise_requests r JOIN federation_expertise_request_audit a USING(space,requester,request_id) WHERE r.space=$1 AND r.requester=$2 AND r.request_id=$3 AND r.request_digest=$4")
                .bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id).bind(sqlx::types::Json(q.digest().unwrap())).fetch_one(&pool).await.unwrap();
            assert_eq!(
                n, 1,
                "gateway received question before its exact consent commit"
            );
        }
        return next
            .run(axum::extract::Request::from_parts(
                parts,
                axum::body::Body::from(bytes),
            ))
            .await;
    }
    next.run(request).await
}
#[tokio::test]
async fn browser_preparation_and_submission_are_scoped_confirmed_and_committed_first() {
    let Some(f) = Fixture::with_expertise(true).await else {
        return;
    };
    let fence = f
        .gateway
        .initialize(f.runtime.space, f.runtime.authority)
        .await
        .unwrap();
    let ad = CapabilityAdvertisement {
        owner: f.runtime.instance,
        collection: RecordId::new(Uuid::new_v4()).unwrap(),
        revision: StateRevision::new(1).unwrap(),
        collection_digest: CanonicalJsonCodec::digest(&"reviewed fixture").unwrap(),
        name: "Fixture expertise".into(),
        description: "Reviewed fixture metadata".into(),
        topics: vec!["recovery".into()],
        modes: vec![AnswerMode::Deterministic],
        requesters: vec![f.runtime.instance],
        model_destinations: vec![],
        output_destinations: vec!["local_human".into(), "synthetic_cloud".into()],
        active: true,
        limits: Default::default(),
    };
    for record in [
        PublicationRecord::Membership {
            member: f.runtime.instance,
            capability: SpaceCapability::AskExpertise,
        },
        PublicationRecord::Membership {
            member: f.runtime.instance,
            capability: SpaceCapability::AnswerExpertise,
        },
        PublicationRecord::Expertise(ad.clone()),
    ] {
        let publisher = if matches!(&record, PublicationRecord::Expertise(_)) {
            f.runtime.instance
        } else {
            f.runtime.authority
        };
        let mut p = intent(f.runtime.space, publisher, record);
        p.envelope.schema_version = 3;
        assert_eq!(
            f.gateway
                .accept(fence, publisher, &p)
                .await
                .unwrap()
                .outcome,
            PublicationOutcome::Applied
        );
    }
    f.publish_fixture_receipts().await;
    f.runtime.sync.catch_up().await.unwrap();
    f.grant(f.root_id, true).await;
    let (status, body) = f
        .request(&f.root, reqwest::Method::GET, "", Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["observed_capabilities"],
        json!(["pull", "publish", "ask_expertise", "answer_expertise"])
    );
    let (status,body)=f.request(&f.root,reqwest::Method::POST,"/grants",json!({"principal":f.writer_id,"capabilities":[SpaceCapability::AskExpertise],"enabled":true})).await;
    assert!(status.is_success(), "{body}");
    let session = format!("session-{}", Uuid::new_v4());
    let csrf = format!("csrf-{}", Uuid::new_v4());
    let session_id = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,expires_at) VALUES($1,$2,$3,$4,now()+interval '1 hour')")
        .bind(session_id).bind(f.writer_id).bind(crate::auth::hash_secret(&session)).bind(crate::auth::hash_secret(&csrf)).execute(&f.pool).await.unwrap();
    let client = reqwest::Client::new();
    let cookies = format!("memory_session={session}; memory_csrf={csrf}");
    let post = |path: &str, body: Value| {
        client
            .post(f.path(path))
            .header("cookie", &cookies)
            .header("origin", "https://local.example.test")
            .header("x-csrf-token", &csrf)
            .json(&body)
            .send()
    };
    let prepare = json!({"request_id":Uuid::new_v4(),"capability":ad.key(),"question":"How is recovery verified?"});
    let get = |path: &str| client.get(f.path(path)).header("cookie", &cookies).send();
    assert_eq!(
        client
            .get(f.path("/capabilities"))
            .header("cookie", &cookies)
            .header("x-memory-expertise-mcp", "1")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let catalog = get("/capabilities?limit=1").await.unwrap();
    assert_eq!(catalog.status(), StatusCode::OK);
    assert_eq!(catalog.headers()["cache-control"], "no-store");
    let catalog: Value = catalog.json().await.unwrap();
    assert_eq!(catalog["capabilities"][0]["name"], ad.name);
    assert_eq!(catalog["next_after"], json!(ad.key()));
    let next = get(&format!(
        "/capabilities?limit=1&after_publisher={}&after_record={}",
        ad.owner.as_uuid(),
        ad.collection.as_uuid()
    ))
    .await
    .unwrap();
    assert_eq!(next.status(), StatusCode::OK);
    let next: Value = next.json().await.unwrap();
    assert_eq!(next["capabilities"], json!([]));
    assert!(next["next_after"].is_null());
    assert_eq!(
        get("/capabilities?limit=0").await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        get(&format!(
            "/capabilities?after_publisher={}",
            f.runtime.instance.as_uuid()
        ))
        .await
        .unwrap()
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.request(&f.writer, reqwest::Method::GET, "/capabilities", json!({}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.request(
            &f.writer,
            reqwest::Method::POST,
            "/questions/prepare",
            prepare.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(f.path("/questions/prepare"))
            .header("cookie", &cookies)
            .json(&prepare)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let token_id: Uuid =
        sqlx::query_scalar("SELECT id FROM auth_service_tokens WHERE principal_id=$1")
            .bind(f.writer_id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let mut bound = (*f.runtime).clone();
    bound.expertise_callers = vec![crate::federation::ExpertiseCaller {
        principal: f.writer_id,
        token: token_id,
        output_destination: "synthetic_cloud".into(),
    }];
    *f.state.federation.write().unwrap() = Some(Arc::new(bound));
    let (status, token_review) = f
        .request(
            &f.writer,
            reqwest::Method::POST,
            "/questions/prepare",
            prepare.clone(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{token_review}");
    assert_eq!(
        token_review["question"]["output_destination"],
        "synthetic_cloud"
    );
    let (status, _) = f
        .request(&f.writer, reqwest::Method::GET, "/collections", Value::Null)
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "cloud tokens must not receive local collection contents"
    );
    let mut self_authorized = prepare.clone();
    self_authorized["output_destination"] = json!("local_human");
    assert_eq!(
        f.request(
            &f.writer,
            reqwest::Method::POST,
            "/questions/prepare",
            self_authorized
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    *f.state.federation.write().unwrap() = Some(f.runtime.clone());
    let token_submit = json!({"confirmed_digest":token_review["digest"],"review":token_review});
    assert_eq!(
        f.request(
            &f.writer,
            reqwest::Method::POST,
            "/questions/submit",
            token_submit
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let response = post("/questions/prepare", prepare).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let review: Value = response.json().await.unwrap();
    assert_eq!(review["question"]["principal"], json!(f.writer_id));
    assert_eq!(review["question"]["output_destination"], "local_human");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM federation_expertise_requests WHERE project_id=$1",
    )
    .bind(f.runtime.project)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
    let submit = json!({"confirmed_digest":review["digest"],"review":review});
    let mut redirected = submit.clone();
    redirected["review"]["question"]["output_destination"] = json!("cloud_model");
    assert_eq!(
        post("/questions/submit", redirected)
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let other = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'fixture','/synthetic')",
    )
    .bind(other)
    .bind(other.to_string())
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'operator')").bind(Uuid::new_v4()).bind(f.writer_id).bind(other).execute(&f.pool).await.unwrap();
    assert_eq!(
        client
            .post(
                f.path("/questions/submit")
                    .replace(&f.slug, &other.to_string())
            )
            .header("cookie", &cookies)
            .header("origin", "https://local.example.test")
            .header("x-csrf-token", &csrf)
            .json(&submit)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let mut readonly = f.state.clone();
    readonly.config.service.read_only = true;
    let (readonly_url, task) = serve(crate::routes::build_http_app(readonly)).await;
    assert_eq!(
        client
            .post(f.path("/questions/submit").replace(&f.url, &readonly_url))
            .header("cookie", &cookies)
            .header("origin", "https://local.example.test")
            .header("x-csrf-token", &csrf)
            .json(&submit)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    task.abort();
    for _ in 0..2 {
        let response = post("/questions/submit", submit.clone()).await.unwrap();
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body["answer"].is_null());
        assert_eq!(body["status"], "queued");
    }
    let result_path = format!(
        "/questions/{}",
        submit["review"]["question"]["request_id"].as_str().unwrap()
    );
    let result = get(&result_path).await.unwrap();
    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(result.headers()["cache-control"], "no-store");
    let result: Value = result.json().await.unwrap();
    assert_eq!(result["status"], "queued");
    assert!(result["answer"].is_null());
    assert_eq!(
        f.request(&f.writer, reqwest::Method::GET, &result_path, json!({}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (status,_)=f.request(&f.root,reqwest::Method::POST,"/grants",json!({"principal":f.root_id,"capabilities":[SpaceCapability::AskExpertise],"enabled":true})).await;
    assert!(status.is_success());
    let other_session = format!("session-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,expires_at) VALUES($1,$2,$3,$4,now()+interval '1 hour')")
        .bind(Uuid::new_v4()).bind(f.root_id).bind(crate::auth::hash_secret(&other_session)).bind(crate::auth::hash_secret("synthetic-other-csrf")).execute(&f.pool).await.unwrap();
    assert_eq!(
        client
            .get(f.path(&result_path))
            .header("cookie", format!("memory_session={other_session}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        get(&format!("/questions/{}", Uuid::new_v4()))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let mut forged = submit.clone();
    forged["review"]["question"]["principal"] = json!(f.root_id);
    assert_eq!(
        post("/questions/submit", forged).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let mut altered = submit.clone();
    altered["review"]["question"]["question"] = json!("Different question");
    assert_eq!(
        post("/questions/submit", altered).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    sqlx::query("DELETE FROM auth_project_memberships WHERE principal_id=$1 AND project_id=$2")
        .bind(f.writer_id)
        .bind(f.runtime.project)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        post("/questions/submit", submit.clone())
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(&result_path).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE auth_web_sessions SET revoked_at=clock_timestamp() WHERE id=$1")
        .bind(session_id)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        get(&result_path).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post("/questions/submit", submit).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
}
