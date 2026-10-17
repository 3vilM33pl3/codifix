// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use clap::Parser;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

type Reply = (u16, String, String);
const COOKIES: &str = "Set-Cookie: codifix_session=mls_synthetic; Path=/; HttpOnly\r\nSet-Cookie: codifix_csrf=mlc_synthetic; Path=/\r\n";

fn config(address: std::net::SocketAddr) -> AppConfig {
    serde_json::from_value(json!({
        "service":{"bind_addr":address.to_string(),"api_token":"synthetic-owner"},
        "database":{"url":"postgresql://unused/unused"}
    }))
    .unwrap()
}
fn owner() -> Value {
    json!({"authenticated":true,"mode":"single_user","read_only":false,
        "principal":{"id":codifix_record::LOCAL_OWNER_PRINCIPAL_ID,
            "kind":"internal","display_name":"Synthetic owner","global_role":"admin"}})
}
async fn server(listener: tokio::net::TcpListener, replies: Vec<Reply>) -> Vec<String> {
    let mut requests = Vec::new();
    for (status, headers, body) in replies {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buffer = [0; 4096];
            let count = socket.read(&mut buffer).await.unwrap();
            assert!(count > 0 && bytes.len() < 16_384);
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(start) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&bytes[..start]).to_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("content-length: ")
                            .and_then(|n| n.parse::<usize>().ok())
                    })
                    .unwrap_or_default();
                if bytes.len() >= start + 4 + length {
                    break;
                }
            }
        }
        requests.push(String::from_utf8(bytes).unwrap());
        socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",body.len()).as_bytes()).await.unwrap();
    }
    requests
}

#[tokio::test]
async fn local_human_session_http_exact_confirmation_csrf_and_revocation_no_remint() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(server(
        listener,
        vec![
            (200, String::new(), owner().to_string()),
            (200, COOKIES.into(), "{}".into()),
            (200, String::new(), "{\"space\":\"synthetic\"}".into()),
            (202, String::new(), "{\"status\":\"queued\"}".into()),
            (403, String::new(), "SECRET_DATABASE_DETAILS".into()),
            (403, String::new(), "SECRET_DATABASE_DETAILS".into()),
        ],
    ));
    let api = FederationClient {
        client: crate::commands::api::local_human_client(config(address))
            .await
            .unwrap(),
        base: format!("http://{address}/v1/projects/fixture/federation"),
        local_human_session: true,
    };
    api.call(Method::GET, "", None).await.unwrap();
    let review = json!({"question":{"request_id":Uuid::new_v4(),"question":"Synthetic exact question\n",
        "output_destination":"local_human"},"digest":{"algorithm":"sha256","value":"a".repeat(64)}});
    let body = expertise::confirmation(review.clone(), "review", "digest", true).unwrap();
    api.call(Method::POST, "/questions/submit", Some(body.clone()))
        .await
        .unwrap();
    // The TUI clones this same client. Neither clone falls back or bootstraps again.
    for _ in 0..2 {
        let error = api
            .clone()
            .call(Method::GET, "/capabilities", None)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("access denied"));
        assert!(!error.contains("SECRET_DATABASE_DETAILS"));
    }
    let requests = server.await.unwrap();
    assert!(requests[0].starts_with("GET /v1/auth/me "));
    assert!(requests[0].contains("x-api-token: synthetic-owner"));
    assert!(requests[1].starts_with("POST /v1/auth/session/bootstrap "));
    assert!(!requests[1].contains("synthetic-owner"));
    for request in &requests[2..] {
        assert!(
            request.contains("cookie: codifix_session=mls_synthetic; codifix_csrf=mlc_synthetic")
        );
        assert!(request.contains("x-csrf-token: mlc_synthetic"));
        assert!(request.contains(&format!("origin: http://{address}\r\n")));
        assert!(!request.contains("x-api-token"));
        assert!(!request.contains("authorization:"));
    }
    let actual: Value =
        serde_json::from_str(requests[3].split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(actual, body);
    assert_eq!(actual["review"], review);
}

#[tokio::test]
async fn local_human_session_rejects_other_auth_without_bootstrap_or_fallback() {
    for response in [
        json!({"mode":"multi_user"}),
        json!({"principal":{"kind":"service_token"}}),
        json!({"principal":{"id":Uuid::new_v4()}}),
        json!({"principal":{"global_role":"reader"}}),
        json!({"authenticated":false}),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut me = owner();
        for (key, value) in response.as_object().unwrap() {
            if key == "principal" {
                for (key, value) in value.as_object().unwrap() {
                    me["principal"][key] = value.clone();
                }
            } else {
                me[key] = value.clone();
            }
        }
        let server = tokio::spawn(server(listener, vec![(200, String::new(), me.to_string())]));
        assert!(
            crate::commands::api::local_human_client(config(address))
                .await
                .is_err()
        );
        assert_eq!(server.await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn local_human_session_denies_nonloopback_before_any_request() {
    for address in [
        "192.0.2.1:1",
        "localhost:1",
        "127.1:1",
        "x@127.0.0.1:1",
        "127.0.0.1:1/path",
    ] {
        let mut config = config("192.0.2.1:1".parse().unwrap());
        config.service.bind_addr = address.into();
        let result = crate::commands::api::local_human_client(config).await;
        assert!(result.unwrap_err().to_string().contains("literal loopback"));
    }
}

#[tokio::test]
async fn local_human_session_rejects_redirects_denials_and_malformed_cookies() {
    for first in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut replies = vec![];
        if !first {
            replies.push((200, String::new(), owner().to_string()));
        }
        replies.push((
            302,
            "Location: http://127.0.0.1:1/forbidden\r\n".into(),
            String::new(),
        ));
        let count = replies.len();
        let server = tokio::spawn(server(listener, replies));
        assert!(
            crate::commands::api::local_human_client(config(address))
                .await
                .is_err()
        );
        assert_eq!(server.await.unwrap().len(), count);
    }
    for headers in [
        String::new(),
        "Set-Cookie: codifix_session=; Path=/\r\n".into(),
        format!("{COOKIES}Set-Cookie: codifix_session=second; Path=/\r\n"),
        COOKIES.replace("mlc_synthetic", "malformed cookie"),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(server(
            listener,
            vec![
                (200, String::new(), owner().to_string()),
                (200, headers, "{}".into()),
            ],
        ));
        assert!(
            crate::commands::api::local_human_client(config(address))
                .await
                .is_err()
        );
        assert_eq!(server.await.unwrap().len(), 2);
    }
}

#[test]
fn local_human_session_is_explicit_for_cli_and_tui() {
    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        args: FederationArgs,
    }
    for command in ["discover", "capabilities", "tui"] {
        assert!(
            !Cli::try_parse_from(["memory", "--project", "fixture", command])
                .unwrap()
                .args
                .local_human_session
        );
        assert!(
            Cli::try_parse_from([
                "memory",
                "--project",
                "fixture",
                "--local-human-session",
                command
            ])
            .unwrap()
            .args
            .local_human_session
        );
    }
}
