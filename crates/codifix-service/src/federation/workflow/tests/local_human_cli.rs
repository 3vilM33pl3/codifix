// SPDX-License-Identifier: AGPL-3.0-or-later
//! Opt-in real CLI + HTTP + PostgreSQL session exchange, synthetic data only.
use super::*;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

struct Files(std::path::PathBuf);
impl Drop for Files {
    fn drop(&mut self) {
        // This directory is created exclusively below, never an operator path.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
#[ignore = "requires explicit MEMORY_LAYER_TEST_CLI_BINARY and disposable PostgreSQL"]
async fn federation_local_human_cli_real_session_and_csrf() {
    let binary = std::fs::canonicalize(
        std::env::var_os("MEMORY_LAYER_TEST_CLI_BINARY")
            .expect("set the explicitly built CLI binary"),
    )
    .unwrap();
    let mut f = Fixture::with_expertise(true)
        .await
        .expect("requires disposable PostgreSQL");
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
            capability: SpaceCapability::AskExpertise,
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
    let mut state = f.state.clone();
    state.config.auth.mode = codifix_record::AuthMode::SingleUser;
    state.api_token = "synthetic-local-owner".into();
    state.config.service.api_token = state.api_token.clone();
    crate::auth::ensure_local_owner_principal(&f.pool)
        .await
        .unwrap();
    let owner = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"memory-layer-local-owner");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    state.config.auth.public_base_url = Some(url.clone());
    let app = crate::routes::build_http_app(state);
    f.tasks.push(tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    }));
    let files = Files(std::env::temp_dir().join(format!("memory-local-human-{}", Uuid::new_v4())));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&files.0)
        .unwrap();
    let config = files.0.join("client.toml");
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&config)
        .unwrap();
    use std::io::Write;
    writeln!(file, "[service]\nbind_addr=\"{address}\"\napi_token=\"synthetic-local-owner\"\n[database]\nurl=\"postgresql://unused/unused\"").unwrap();
    drop(file);
    let run = |args: Vec<String>| {
        let binary = binary.clone();
        let config = config.clone();
        let directory = files.0.clone();
        let project = f.slug.clone();
        async move {
            tokio::time::timeout(
                std::time::Duration::from_secs(30),
                tokio::process::Command::new(binary)
                    .env_clear()
                    .env("MEMORY_LAYER_PROFILE", "prod")
                    .env("HOME", &directory)
                    .current_dir(directory)
                    .arg("--config")
                    .arg(config)
                    .args(["federation", "--project", &project])
                    .args(args)
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .unwrap()
            .unwrap()
        }
    };
    let output = run(vec![
        "--local-human-session".into(),
        "grant".into(),
        "--principal".into(),
        owner.to_string(),
        "--capabilities".into(),
        "pull,ask-expertise".into(),
        "--confirm".into(),
    ])
    .await;
    assert!(
        output.status.success(),
        "synthetic local-human grant failed"
    );
    let output = run(vec!["--local-human-session".into(), "capabilities".into()]).await;
    assert!(
        output.status.success(),
        "synthetic local-human capability read failed"
    );
    assert!(serde_json::from_slice::<Value>(&output.stdout).is_ok());
    assert!(
        !run(vec!["capabilities".into()]).await.status.success(),
        "ordinary installation token must remain ineligible for expertise"
    );

    // Check the real server still enforces CSRF, exact Origin, MCP separation
    // and revocation for these same browser credential semantics.
    let client = reqwest::Client::new();
    let bootstrap = client
        .post(format!("{url}/v1/auth/session/bootstrap"))
        .send()
        .await
        .unwrap();
    assert_eq!(bootstrap.status(), StatusCode::OK);
    let cookies = bootstrap
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let csrf = cookies
        .split("; ")
        .find_map(|v| v.strip_prefix("memory_csrf="))
        .unwrap();
    let session = cookies
        .split("; ")
        .find_map(|v| v.strip_prefix("memory_session="))
        .unwrap();
    let path = format!(
        "{url}/v1/projects/{}/federation/{}",
        f.slug,
        f.runtime.space.as_uuid()
    );
    let grant = json!({"principal":owner,"capabilities":["pull","ask_expertise"],"enabled":true});
    for origin in [None, Some("https://wrong.invalid")] {
        let mut request = client
            .post(format!("{path}/grants"))
            .header("cookie", &cookies)
            .json(&grant);
        if let Some(origin) = origin {
            request = request
                .header("origin", origin)
                .header("x-csrf-token", csrf);
        }
        assert_eq!(
            request.send().await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        client
            .get(format!("{path}/capabilities"))
            .header("cookie", &cookies)
            .header("x-memory-expertise-mcp", "1")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE auth_web_sessions SET revoked_at=now() WHERE session_hash=$1")
        .bind(crate::auth::hash_secret(session))
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        client
            .get(format!("{path}/capabilities"))
            .header("cookie", cookies)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
}
