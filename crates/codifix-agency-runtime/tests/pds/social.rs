use super::*;
use serde::Deserialize;
use std::{fs, path::PathBuf, process::Command};

#[derive(Deserialize)]
struct Pds {
    base: String,
    repository: String,
    token: String,
    signing_key: String,
    recipient_did: String,
}

fn pds() -> (Pds, PathBuf) {
    let path = PathBuf::from(std::env::var_os("CODIFIX_FEDERATION_PDS_FIXTURE").unwrap());
    let pds = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    (pds, path.parent().unwrap().to_owned())
}

#[tokio::test]
#[ignore = "requires tests/federation-pds/run.mjs --agency"]
async fn public_capabilities_follow_operator_availability_and_expiry() {
    let (pds, directory) = pds();
    let f = fixture().await.unwrap();
    let config = directory.join("agency-config.json");
    fs::write(&config, serde_json::to_vec(&f.cfg).unwrap()).unwrap();
    let rkey = format!("{}-operations", f.cfg.instance_id.simple());
    let execute = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_codifix-agency"))
            .arg("--config")
            .arg(&config)
            .args(args)
            .env("AGENCY_FIXTURE_ACCESS_TOKEN", &pds.token)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "agency CLI failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<Value>(&out.stdout).unwrap()
    };
    let toggle = |on: bool, expiry: Option<&str>| {
        let mut args = vec![
            "availability",
            "--pds",
            &pds.base,
            "--did",
            &pds.repository,
            "--token-env",
            "AGENCY_FIXTURE_ACCESS_TOKEN",
            "--capability",
            "inspect",
            "--confirm-public-disclosure",
            if on { "--on" } else { "--off" },
        ];
        if let Some(expiry) = expiry {
            args.extend(["--expires-at", expiry]);
        }
        execute(&args)
    };
    let discover = || {
        execute(&[
            "discover",
            "--pds",
            &pds.base,
            "--did",
            &pds.repository,
            "--record-key",
            &rkey,
        ])
    };
    toggle(true, None);
    assert_eq!(discover()["capabilities"], json!(["inspect"]));
    toggle(false, None);
    assert_eq!(discover()["capabilities"], json!([]));
    let expiry = Utc::now() + Duration::seconds(3);
    toggle(true, Some(&expiry.to_rfc3339()));
    assert_eq!(discover()["capabilities"], json!(["inspect"]));
    let delay = (expiry - Utc::now()).to_std().unwrap_or_default();
    tokio::time::sleep(delay).await;
    assert_eq!(discover()["capabilities"], json!([]));
    f.cleanup().await;
}

impl Pds {
    async fn service_token(&self, audience: &str, method: &str) -> String {
        let audience = format!("{audience}#codifix_agency");
        let response = reqwest::Client::new()
            .get(format!(
                "{}/xrpc/com.atproto.server.getServiceAuth",
                self.base
            ))
            .bearer_auth(&self.token)
            .query(&[("aud", audience.as_str()), ("lxm", method)])
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        response["token"].as_str().unwrap().to_owned()
    }
}

#[tokio::test]
#[ignore = "requires tests/federation-pds/run.mjs --agency"]
async fn private_messages_use_pds_credentials_and_respect_recipient_moderation() {
    use codifix_agency_runtime::social::{SocialConfig, SocialPeer};
    const SEND: &str = "dev.codifix.agency.sendMessage";
    const GET: &str = "dev.codifix.agency.getTask";
    const SEND_PATH: &str = "/xrpc/dev.codifix.agency.sendMessage";
    let (pds, _) = pds();
    let did = &pds.recipient_did;
    let mut f = fixture().await.unwrap();
    f.server.abort();
    let peer = f
        .cfg
        .peers
        .iter_mut()
        .find(|peer| peer.id == "member")
        .unwrap();
    peer.instance_id = Uuid::new_v4();
    f.cfg.social = Some(SocialConfig {
        did: did.clone(),
        pds: pds.base.clone(),
        access_token_env: "UNUSED_RECEIVER_PDS_TOKEN".into(),
        access_token_file: None,
        peers: BTreeMap::from([(
            "member".into(),
            SocialPeer {
                incident_conversation: None,
                notice_inbox: None,
                did: pds.repository.clone(),
                signing_key: pds.signing_key.clone(),
            },
        )]),
    });
    f.cfg.validate().unwrap();
    let operator = f.cfg.peers.iter().find(|p| p.operator).unwrap().clone();
    let auth = Auth::from_credentials(vec![(operator, OPERATOR_TOKEN.into())]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    f.url = format!("http://{}", listener.local_addr().unwrap());
    let app = router(App {
        store: f.store.clone(),
        config: Arc::new(f.cfg.clone()),
        auth,
    });
    f.server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut message = f.message();
    message["message"]["parts"][0]["data"]["objective"] =
        json!("PRIVATE_SOCIAL_SENTINEL observe service");
    for scoped_for_another_call in [
        pds.service_token(did, GET).await,
        pds.service_token("did:web:unrelated.federation.example.com", SEND)
            .await,
    ] {
        assert_eq!(
            f.request(
                Method::POST,
                SEND_PATH,
                Some(&scoped_for_another_call),
                None,
                Some(message.clone())
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    let token = pds.service_token(did, SEND).await;
    let (status, result) = f
        .request(
            Method::POST,
            SEND_PATH,
            Some(&token),
            None,
            Some(message.clone()),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let id = result["task"]["id"].as_str().unwrap();
    assert_eq!(
        f.request(
            Method::POST,
            SEND_PATH,
            Some(&token),
            None,
            Some(message.clone())
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let retry = pds.service_token(did, SEND).await;
    let (status, repeated) = f
        .request(
            Method::POST,
            SEND_PATH,
            Some(&retry),
            None,
            Some(message.clone()),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated["task"]["id"], id);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM agency_tasks")
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        1
    );
    let mut conflicting = message;
    conflicting["message"]["parts"][0]["data"]["objective"] = json!("Changed after acceptance");
    let conflict_token = pds.service_token(did, SEND).await;
    assert_eq!(
        f.request(
            Method::POST,
            SEND_PATH,
            Some(&conflict_token),
            None,
            Some(conflicting)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert!(worker::run_once(&f.store, &f.cfg).await.unwrap());
    let path = format!("/xrpc/{GET}?id={id}");
    let get = || async {
        let token = pds.service_token(did, GET).await;
        f.request(Method::GET, &path, Some(&token), None, None)
            .await
    };
    let (status, result) = get().await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["status"]["state"], "TASK_STATE_COMPLETED");
    let moderation = |action: &str, until: Option<chrono::DateTime<Utc>>| {
        f.request(
            Method::POST,
            "/v1/social/moderation",
            Some(OPERATOR_TOKEN),
            None,
            Some(json!({"peer_id":"member","action":action,"until":until})),
        )
    };
    let until = Utc::now() + Duration::seconds(3);
    assert_eq!(moderation("ban", Some(until)).await.0, StatusCode::OK);
    assert_eq!(get().await.0, StatusCode::FORBIDDEN);
    tokio::time::sleep((until - Utc::now()).to_std().unwrap_or_default()).await;
    assert_eq!(get().await.0, StatusCode::OK);
    assert_eq!(moderation("remove", None).await.0, StatusCode::OK);
    assert_eq!(get().await.0, StatusCode::FORBIDDEN);
    assert_eq!(moderation("allow", None).await.0, StatusCode::OK);
    assert_eq!(get().await.0, StatusCode::OK);
    f.cleanup().await;
}
