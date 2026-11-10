//! Stdin/stdout bridge for the isolated service test. Compiled only in tests.
use super::{endpoint, is_local, Session, Transport};
use serde::Deserialize;
use serde_json::Value;
use std::io;

#[derive(Deserialize)]
struct Request {
    origin: String,
    token: Option<String>,
    path: String,
    method: String,
    body: Option<Value>,
}

#[tokio::test]
#[ignore = "driven by scripts/test-inbox-candidate.sh"]
async fn native_inbox_request() {
    let request: Request = serde_json::from_reader(io::stdin().lock()).unwrap();
    let origin = endpoint(&request.origin).unwrap();
    assert_eq!(origin.host_str(), Some("127.0.0.1"));
    assert!(is_local(&origin));
    assert!(request.token.as_ref().is_none_or(|t| t.starts_with("mld_")));

    let transport = Transport::default();
    let mut session = Session::new(origin, request.token).unwrap();
    session.fence = Some((transport.1.clone(), 0));
    transport.0.lock().unwrap().session = Some(session);
    let result = transport
        .api_request(&request.path, &request.method, request.body)
        .await;
    println!(
        "INBOX_NATIVE_RESPONSE {}",
        serde_json::to_string(&result).unwrap()
    );
}
