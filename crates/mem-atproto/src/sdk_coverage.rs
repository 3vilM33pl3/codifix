// SPDX-License-Identifier: AGPL-3.0-or-later
//! SDK request-contract probes only: no sockets, credentials, signing or OAuth.

use atrium_api::{client::AtpServiceClient, com::atproto::sync::get_repo};
use atrium_xrpc::{HttpClient, XrpcClient, http, types::AuthorizationToken};
use futures::executor::block_on;
use http::{Request, Response};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct StubClient {
    requests: Arc<Mutex<Vec<Request<Vec<u8>>>>>,
    responses: Arc<Mutex<VecDeque<Response<Vec<u8>>>>>,
}

impl StubClient {
    fn new(responses: Vec<Response<Vec<u8>>>) -> Self {
        Self {
            requests: Arc::default(),
            responses: Arc::new(Mutex::new(responses.into())),
        }
    }
}

impl HttpClient for StubClient {
    async fn send_http(
        &self,
        request: Request<Vec<u8>>,
    ) -> Result<Response<Vec<u8>>, Box<dyn std::error::Error + Send + Sync>> {
        self.requests.lock().unwrap().push(request);
        Ok(self.responses.lock().unwrap().pop_front().unwrap())
    }
}

impl XrpcClient for StubClient {
    fn base_uri(&self) -> String {
        "https://offline-sdk.example.test".into()
    }

    async fn authorization_token(&self, refresh: bool) -> Option<AuthorizationToken> {
        Some(AuthorizationToken::Bearer(
            if refresh {
                "fixture-refresh"
            } else {
                "fixture-access"
            }
            .into(),
        ))
    }
}

fn response(content_type: &str, bytes: &[u8]) -> Response<Vec<u8>> {
    Response::builder()
        .status(200)
        .header(http::header::CONTENT_TYPE, content_type)
        .body(bytes.to_vec())
        .unwrap()
}

fn repository() -> get_repo::Parameters {
    get_repo::ParametersData {
        did: "did:web:memory.example.test".parse().unwrap(),
        since: None,
    }
    .into()
}

#[test]
fn pinned_sdk_supports_binary_repository_export_and_import_without_a_runtime() {
    block_on(async {
        // Deliberately opaque test bytes: this checks transport preservation,
        // not CAR validity. Signed CAR verification lives in the PDS gate.
        let bytes = b"\0opaque CAR transport fixture\xff";
        let client = StubClient::new(vec![
            response("application/vnd.ipld.car", bytes),
            response("application/octet-stream", b""),
        ]);
        let api = AtpServiceClient::new(client.clone());
        let exported = api
            .service
            .com
            .atproto
            .sync
            .get_repo(repository())
            .await
            .unwrap();
        assert_eq!(exported, bytes);
        api.service
            .com
            .atproto
            .repo
            .import_repo(exported)
            .await
            .unwrap();
        let requests = client.requests.lock().unwrap();
        assert_eq!(requests[0].method(), http::Method::GET);
        assert_eq!(requests[0].uri().path(), "/xrpc/com.atproto.sync.getRepo");
        assert_eq!(
            requests[0].headers()[http::header::AUTHORIZATION],
            "Bearer fixture-access"
        );
        assert_eq!(requests[1].method(), http::Method::POST);
        assert_eq!(
            requests[1].uri().path(),
            "/xrpc/com.atproto.repo.importRepo"
        );
        assert_eq!(
            requests[1].headers()[http::header::CONTENT_TYPE],
            "application/vnd.ipld.car"
        );
        assert_eq!(requests[1].body(), bytes);
        // Atrium leaves HTTP framing to HttpClient. A real client must supply
        // Content-Length and enforce endpoint/redirect/timeout/size policy.
        assert!(
            !requests[1]
                .headers()
                .contains_key(http::header::CONTENT_LENGTH)
        );
    });
}

#[test]
fn refresh_uses_refresh_credential_and_activation_has_no_json_body() {
    block_on(async {
        let session = br#"{"did":"did:web:memory.example.test","handle":"memory.example.test","accessJwt":"new-fixture-access","refreshJwt":"new-fixture-refresh"}"#;
        let client = StubClient::new(vec![
            response("application/json", session),
            response("application/octet-stream", b""),
        ]);
        let api = AtpServiceClient::new(client.clone());
        let refreshed = api
            .service
            .com
            .atproto
            .server
            .refresh_session()
            .await
            .unwrap();
        assert_eq!(refreshed.data.did.as_ref(), "did:web:memory.example.test");
        api.service
            .com
            .atproto
            .server
            .activate_account()
            .await
            .unwrap();
        let requests = client.requests.lock().unwrap();
        assert_eq!(
            requests[0].headers()[http::header::AUTHORIZATION],
            "Bearer fixture-refresh"
        );
        assert_eq!(
            requests[1].headers()[http::header::AUTHORIZATION],
            "Bearer fixture-access"
        );
        for request in requests.iter() {
            assert_eq!(request.method(), http::Method::POST);
            assert!(request.body().is_empty());
            assert!(!request.headers().contains_key(http::header::CONTENT_TYPE));
        }
    });
}

#[test]
fn authentication_failure_is_not_retried_as_an_anonymous_read() {
    block_on(async {
        let denied = Response::builder()
            .status(401)
            .header(http::header::WWW_AUTHENTICATE, "Bearer")
            .body(Vec::new())
            .unwrap();
        let client = StubClient::new(vec![denied]);
        let api = AtpServiceClient::new(client.clone());
        assert!(matches!(
            api.service.com.atproto.sync.get_repo(repository()).await,
            Err(atrium_xrpc::Error::Authentication(_))
        ));
        assert_eq!(client.requests.lock().unwrap().len(), 1);
    });
}

#[test]
fn current_profile_does_not_silently_accept_native_spaces_addressing() {
    let native = "at://did:web:authority.example.test/space/dev.memory-layer.unspecced.space/demo/did:web:author.example.test/dev.memory-layer.unspecced.memoryVersion/key";
    assert!(native.parse::<crate::AtUri>().is_err());
}
