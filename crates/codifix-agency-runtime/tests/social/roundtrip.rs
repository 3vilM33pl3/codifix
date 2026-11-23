use super::*;
use atrium_crypto::keypair::{Did, Secp256k1Keypair};
use axum::{extract::Query, http::HeaderMap, routing::get};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use codifix_agency_runtime::social::{SocialConfig, SocialPeer};

pub(super) async fn install(home: &mut Config, remote: &mut Config) -> JoinHandle<()> {
    let home_did = "did:web:home.test";
    let remote_did = "did:web:remote.test";
    let signing = Arc::new(Secp256k1Keypair::import(&[1; 32]).unwrap());
    let verifying = signing.did();
    let issue = move |headers: HeaderMap, Query(q): Query<BTreeMap<String, String>>| {
        let signer = signing.clone();
        async move {
            assert_eq!(
                headers["authorization"],
                "Bearer synthetic-human-remote-inbox-peer-only-token"
            );
            assert_eq!(q["aud"], "did:web:remote.test#codifix_agency");
            let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256K","typ":"JWT"}"#);
            let claims = json!({"iss":home_did,"aud":q["aud"],"lxm":q["lxm"],"jti":Uuid::new_v4(),"exp":Utc::now().timestamp()+60});
            let body = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
            let signed = format!("{header}.{body}");
            let sig = URL_SAFE_NO_PAD.encode(signer.sign(signed.as_bytes()).unwrap());
            Json(json!({"token":format!("{signed}.{sig}")}))
        }
    };
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pds = format!("http://{}", tcp.local_addr().unwrap());
    home.social = Some(SocialConfig {
        did: home_did.into(),
        pds: pds.clone(),
        access_token_env: "CODIFIX_INBOX_TEST_PEER_TOKEN".into(),
        peers: BTreeMap::from([(
            "remote".into(),
            SocialPeer {
                incident_conversation: None,
                notice_inbox: None,
                did: remote_did.into(),
                signing_key: Secp256k1Keypair::import(&[2; 32]).unwrap().did(),
            },
        )]),
    });
    remote.social = Some(SocialConfig {
        did: remote_did.into(),
        pds,
        access_token_env: "UNUSED_REMOTE_PDS".into(),
        peers: BTreeMap::from([(
            "human-home".into(),
            SocialPeer {
                incident_conversation: None,
                notice_inbox: None,
                did: home_did.into(),
                signing_key: verifying,
            },
        )]),
    });
    tokio::spawn(async move {
        axum::serve(
            tcp,
            Router::new().route("/xrpc/com.atproto.server.getServiceAuth", get(issue)),
        )
        .await
        .unwrap()
    })
}

pub(super) async fn group(
    inbox: &codifix_inbox::store::Store,
    human: Uuid,
    chat: Uuid,
    original: &codifix_inbox::InvocationGrant,
    home: &Fixture,
    remote: &Fixture,
) {
    // A second contact uses the home specialist. Group sends address both
    // specialists; the remote member's reply must still use XRPC.
    let mut local = original.clone();
    local.id = Uuid::new_v4();
    local.contact.id = Uuid::new_v4();
    local.contact.instance_id = home.cfg.instance_id;
    local.contact.remote = false;
    local.contact.mandate_id = Some(home.mandate.id);
    local.contact.display_name = "Home planner".into();
    local.request = serde_json::to_value(home.request(home.cfg.instance_id)).unwrap();
    inbox.install_grant(&local).await.unwrap();
    let detail = inbox
        .add_member(
            human,
            chat,
            &codifix_inbox::AddMember {
                client_request_id: Uuid::new_v4(),
                specialist_id: local.contact.id,
                summary: "Group discussion".into(),
                selected_message_ids: vec![],
                confirmed: true,
            },
        )
        .await
        .unwrap();
    let root = Uuid::new_v4();
    let replies_before = remote.provider.lock().unwrap().calls.len();
    inbox
        .send(
            human,
            chat,
            &codifix_inbox::SendMessage {
                client_message_id: root,
                body: "Both planners, please report.".into(),
                format: "text".into(),
                artifact: None,
                recipient_member_ids: detail
                    .members
                    .iter()
                    .filter(|m| m.active)
                    .map(|m| m.id)
                    .collect(),
            },
        )
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            worker::run_once(&home.store, &home.cfg).await.unwrap();
            let detail = inbox.detail(human, chat).await.unwrap();
            if detail
                .tasks
                .iter()
                .filter(|t| {
                    t.root_id == root && matches!(t.status.as_str(), "completed" | "input_required")
                })
                .count()
                == 2
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        remote.provider.lock().unwrap().calls.len(),
        replies_before + 1
    );
    assert_eq!(home.provider.lock().unwrap().calls.len(), 1);
    let turns: i32 = sqlx::query_scalar("SELECT turns FROM inbox_roots WHERE id=$1")
        .bind(root)
        .fetch_one(&home.db.pool)
        .await
        .unwrap();
    assert_eq!(turns, 2);
}

#[tokio::test]
async fn ban_after_queue_prevents_model_work_and_expires() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let peer = Peer {
        id: "source".into(),
        instance_id: Uuid::new_v4(),
        operator: false,
        mandates: BTreeSet::from([f.mandate.id]),
        token_env: String::new(),
        endpoint: None,
        outbound_token_env: None,
    };
    let first = f
        .store
        .submit(&f.request(peer.instance_id), &peer)
        .await
        .unwrap();
    sqlx::query("INSERT INTO agency_social_moderation(instance_id,peer_id,banned_until) VALUES($1,$2,clock_timestamp()+interval '1 hour')")
        .bind(f.cfg.instance_id).bind(&peer.id).execute(&f.db.pool).await.unwrap();
    assert!(
        f.store
            .submit(&f.request(peer.instance_id), &peer)
            .await
            .is_err()
    );
    worker::run_once(&f.store, &f.cfg).await.unwrap();
    assert_ne!(
        f.store
            .get_internal(first.contract.id)
            .await
            .unwrap()
            .contract
            .status,
        TaskStatus::Completed
    );
    assert!(f.provider.lock().unwrap().calls.is_empty());
    sqlx::query("UPDATE agency_social_moderation SET banned_until=clock_timestamp()-interval '1 second' WHERE instance_id=$1 AND peer_id=$2")
        .bind(f.cfg.instance_id).bind(&peer.id).execute(&f.db.pool).await.unwrap();
    assert!(
        f.store
            .submit(&f.request(peer.instance_id), &peer)
            .await
            .is_ok()
    );
    f.cleanup().await;
}
