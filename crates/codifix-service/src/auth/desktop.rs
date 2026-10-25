// SPDX-License-Identifier: AGPL-3.0-or-later
//! Human native login. Loopback URLs carry only a one-time PKCE code and state.
use super::{
    AuthenticatedPrincipal, CookiePolicy, Credential, CredentialSource, credential_from_headers,
    hash_secret, random_secret,
};
use crate::{ApiError, AppState};
use axum::{
    Extension, Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use codifix_record::{
    AuthDesktopExchangeRequest, AuthDesktopExchangeResponse, AuthPrincipalKind, PermissionSet,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

fn bad() -> ApiError {
    ApiError::status_message(StatusCode::BAD_REQUEST, "invalid desktop login parameters")
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DesktopContext {
    redirect_uri: String,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
}

impl DesktopContext {
    pub(super) fn validate(&self) -> Result<(), ApiError> {
        validate_redirect(&self.redirect_uri)?;
        if self.state.len() != 64
            || !self
                .state
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.code_challenge_method != "S256"
            || self.code_challenge.len() != 43
            || URL_SAFE_NO_PAD
                .decode(&self.code_challenge)
                .ok()
                .is_none_or(|b| b.len() != 32 || URL_SAFE_NO_PAD.encode(b) != self.code_challenge)
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(super) fn redirect(&self, key: &str, value: &str) -> Result<Response, ApiError> {
        let mut url = validate_redirect(&self.redirect_uri)?;
        url.query_pairs_mut()
            .append_pair(key, value)
            .append_pair("state", &self.state);
        Ok(no_store(Redirect::to(url.as_str()).into_response()))
    }
}

fn validate_redirect(value: &str) -> Result<reqwest::Url, ApiError> {
    if value.len() > 128 {
        return Err(bad());
    }
    let url = reqwest::Url::parse(value).map_err(|_| bad())?;
    // Require the exact serialized authority and path; URL normalization must
    // not admit alternate numeric hosts, encoded paths or dot segments.
    let port = url
        .port_or_known_default()
        .filter(|p| *p > 0)
        .ok_or_else(bad)?;
    if value != format!("http://127.0.0.1:{port}/callback") {
        return Err(bad());
    }
    Ok(url)
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

pub(crate) async fn desktop_login(
    State(state): State<AppState>,
    Query(context): Query<DesktopContext>,
) -> Result<Response, ApiError> {
    context.validate()?;
    Ok(no_store(
        super::oidc::start_oidc_login(&state, None, Some(context)).await?,
    ))
}

pub(super) async fn issue_desktop_code_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    principal: Uuid,
    context: &DesktopContext,
) -> Result<String, ApiError> {
    let code = random_secret(32);
    sqlx::query("DELETE FROM auth_desktop_codes WHERE code_hash IN (SELECT code_hash FROM auth_desktop_codes WHERE expires_at<=now() LIMIT 256)")
        .execute(&mut **tx).await.map_err(ApiError::sql)?;
    sqlx::query("INSERT INTO auth_desktop_codes(code_hash,principal_id,code_challenge,redirect_uri,expires_at) VALUES($1,$2,$3,$4,now()+interval '60 seconds')")
        .bind(hash_secret(&code)).bind(principal).bind(&context.code_challenge).bind(&context.redirect_uri).execute(&mut **tx).await.map_err(ApiError::sql)?;
    Ok(code)
}

pub(crate) async fn desktop_exchange(
    State(state): State<AppState>,
    Json(request): Json<AuthDesktopExchangeRequest>,
) -> Result<Response, ApiError> {
    validate_redirect(&request.redirect_uri)?;
    if request.code.len() != 43
        || !request
            .code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        || !(43..=128).contains(&request.code_verifier.len())
        || !request
            .code_verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
    {
        return Err(bad());
    }
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(request.code_verifier.as_bytes()));
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    // Lock the human first, matching identity-disable -> session ordering.
    let principal: Uuid = sqlx::query_scalar("SELECT p.id FROM auth_principals p JOIN auth_desktop_codes c ON c.principal_id=p.id WHERE c.code_hash=$1 AND p.disabled_at IS NULL AND p.kind='human_oidc' FOR SHARE OF p")
        .bind(hash_secret(&request.code)).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?
        .ok_or_else(|| ApiError::unauthorized("invalid or expired desktop login code"))?;
    let consumed: Option<Uuid> = sqlx::query_scalar("DELETE FROM auth_desktop_codes WHERE code_hash=$1 AND principal_id=$2 AND code_challenge=$3 AND redirect_uri=$4 AND expires_at>clock_timestamp() RETURNING principal_id")
        .bind(hash_secret(&request.code)).bind(principal).bind(challenge).bind(&request.redirect_uri)
        .fetch_optional(&mut *tx).await.map_err(ApiError::sql)?;
    if consumed.is_none() {
        return Err(ApiError::unauthorized(
            "invalid or expired desktop login code",
        ));
    }
    let token = format!("mld_{}", random_secret(32));
    let expires_at = Utc::now()
        + Duration::from_std(state.config.auth.session_ttl)
            .map_err(|_| ApiError::internal("auth session ttl is too large"))?;
    sqlx::query("WITH created AS (INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,session_kind,expires_at) VALUES($1,$2,$3,$4,'desktop',$5) RETURNING id,principal_id) INSERT INTO auth_audit_events(id,actor_principal_id,target_principal_id,event_type,outcome,metadata_json) SELECT gen_random_uuid(),principal_id,principal_id,'desktop.session.create','success',jsonb_build_object('session_id',id) FROM created")
        .bind(Uuid::new_v4()).bind(principal).bind(hash_secret(&token)).bind(hash_secret(&random_secret(32))).bind(expires_at).execute(&mut *tx).await.map_err(ApiError::sql)?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(no_store(
        Json(AuthDesktopExchangeResponse {
            token,
            principal_id: principal,
            expires_at,
        })
        .into_response(),
    ))
}

/// Narrow expired/revoked proof is used only by this exact logout route. It
/// never grants permissions or rehabilitates the session for ordinary requests.
pub(super) async fn desktop_logout_proof(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<AuthenticatedPrincipal>, ApiError> {
    let Credential::NativeSession(secret) = credential_from_headers(headers, CookiePolicy::Allow)?
    else {
        return Err(ApiError::unauthorized("native bearer session required"));
    };
    if secret.len() != 47 {
        return Err(ApiError::unauthorized("native bearer session required"));
    }
    let row = sqlx::query("SELECT s.id,s.principal_id FROM auth_web_sessions s JOIN auth_principals p ON p.id=s.principal_id WHERE s.session_hash=$1 AND s.session_kind='desktop' AND p.kind='human_oidc'")
        .bind(hash_secret(&secret)).fetch_optional(&state.pool()?).await.map_err(ApiError::sql)?.ok_or_else(|| ApiError::unauthorized("native bearer session required"))?;
    Ok(Some(AuthenticatedPrincipal {
        id: row.try_get("principal_id").map_err(ApiError::sql)?,
        kind: AuthPrincipalKind::HumanOidc,
        display_name: String::new(),
        email: None,
        issuer: None,
        subject: None,
        groups: vec![],
        global_role: None,
        global: PermissionSet::EMPTY,
        project_roles: Default::default(),
        credential_source: CredentialSource::NativeSession,
        token_id: None,
        session_id: Some(row.try_get("id").map_err(ApiError::sql)?),
        session_csrf_hash: None,
    }))
}

pub(crate) async fn desktop_logout(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
) -> Result<Response, ApiError> {
    if principal.credential_source != CredentialSource::NativeSession {
        return Err(ApiError::unauthorized("native bearer session required"));
    }
    sqlx::query("WITH revoked AS (UPDATE auth_web_sessions SET revoked_at=clock_timestamp() WHERE id=$1 AND principal_id=$2 AND session_kind='desktop' AND revoked_at IS NULL RETURNING id,principal_id) INSERT INTO auth_audit_events(id,actor_principal_id,target_principal_id,event_type,outcome,metadata_json) SELECT gen_random_uuid(),principal_id,principal_id,'desktop.session.revoke','success',jsonb_build_object('session_id',id) FROM revoked")
        .bind(principal.session_id).bind(principal.id).execute(&state.pool()?).await.map_err(ApiError::sql)?;
    Ok(no_store(
        Json(serde_json::json!({"logged_out":true})).into_response(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::ConnectOptions;
    use std::{
        net::SocketAddr,
        sync::{Arc, Mutex},
    };

    fn context() -> DesktopContext {
        DesktopContext {
            redirect_uri: "http://127.0.0.1:42001/callback".into(),
            state: "a".repeat(64),
            code_challenge: URL_SAFE_NO_PAD.encode(Sha256::digest("b".repeat(64).as_bytes())),
            code_challenge_method: "S256".into(),
        }
    }
    #[test]
    fn validates_exact_loopback_state_and_challenge() {
        context().validate().unwrap();
        for uri in [
            "https://127.0.0.1:42001/callback",
            "http://localhost:42001/callback",
            "http://127.0.0.2:42001/callback",
            "http://127.0.0.1:0/callback",
            "http://127.0.0.1/callback",
            "http://127.0.0.1:42001/callback?x=1",
            "http://127.0.0.1:42001/callback#frag",
            "http://user@127.0.0.1:42001/callback",
            "http://127.0.0.1:42001/foo/../callback",
            "http://2130706433:42001/callback",
            "http://127.0.0.1:42001/%63allback",
        ] {
            let mut c = context();
            c.redirect_uri = uri.into();
            assert!(c.validate().is_err(), "{uri}");
        }
        for value in ["A".repeat(64), "a".repeat(63), "g".repeat(64)] {
            let mut c = context();
            c.state = value;
            assert!(c.validate().is_err());
        }
        let mut c = context();
        c.code_challenge_method = "plain".into();
        assert!(c.validate().is_err());
        let mut c = context();
        c.code_challenge = "a".repeat(43);
        assert!(c.validate().is_err());
    }

    #[tokio::test]
    async fn pkce_exchange_sessions_revocation_and_cookie_boundaries() {
        let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let config=serde_json::from_value(json!({"database":{"url":db.pool.connect_options().to_url_lossy().as_str()},"service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-owner"},"auth":{"mode":"single_user","public_base_url":"https://company.example.test","oidc":{"issuer_url":"https://auth.example.test","client_id":"test"}},"offline":{"enabled":false},"cluster":{"enabled":false},"background_work":{"enabled":false},"automation":{"enabled":false},"embeddings":{"enabled":false,"create_enabled":false},"consolidation":{"enabled":false},"reinforcement":{"enabled":false},"llm":{"provider":"ollama","model":"disabled","base_url":"http://127.0.0.1:1"}})).unwrap();
        let mut state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
            .await
            .unwrap();
        state.config.auth.mode = codifix_record::AuthMode::MultiUser;
        let mut tx = db.pool.begin().await.unwrap();
        let human = super::super::oidc::upsert_human_principal_tx(
            &mut tx,
            "https://auth.example.test",
            "person",
            "Person",
            None,
            &[],
        )
        .await
        .unwrap();
        let code = issue_desktop_code_tx(&mut tx, human, &context())
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = crate::routes::build_http_app(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap()
        });
        let client = reqwest::Client::new();
        let exchange = format!("{base}/v1/auth/desktop/exchange");
        let request = json!({"code":code,"code_verifier":"b".repeat(64),"redirect_uri":context().redirect_uri});
        for changed in [
            json!({"code_verifier":"c".repeat(64)}),
            json!({"redirect_uri":"http://127.0.0.1:42002/callback"}),
        ] {
            let mut wrong = request.clone();
            wrong
                .as_object_mut()
                .unwrap()
                .extend(changed.as_object().unwrap().clone());
            assert_eq!(
                client
                    .post(&exchange)
                    .json(&wrong)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
        // Protected audit failure rolls back both code consumption and session.
        sqlx::raw_sql("CREATE FUNCTION reject_native_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='desktop.session.create' THEN RAISE EXCEPTION 'synthetic audit failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_native_audit BEFORE INSERT ON auth_audit_events FOR EACH ROW EXECUTE FUNCTION reject_native_audit()")
            .execute(&db.pool).await.unwrap();
        assert_eq!(
            client
                .post(&exchange)
                .json(&request)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        let remaining: i64 =
            sqlx::query_scalar("SELECT count(*) FROM auth_desktop_codes WHERE principal_id=$1")
                .bind(human)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(remaining, 1);
        let sessions: i64 =
            sqlx::query_scalar("SELECT count(*) FROM auth_web_sessions WHERE principal_id=$1")
                .bind(human)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(sessions, 0);
        sqlx::raw_sql("DROP TRIGGER reject_native_audit ON auth_audit_events; DROP FUNCTION reject_native_audit()").execute(&db.pool).await.unwrap();
        let response = client.post(&exchange).json(&request).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!response.headers().contains_key(header::SET_COOKIE));
        let session: AuthDesktopExchangeResponse = response.json().await.unwrap();
        assert!(session.token.starts_with("mld_"));
        assert_eq!(session.principal_id, human);
        assert_eq!(
            client
                .post(&exchange)
                .json(&request)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let me = format!("{base}/v1/auth/me");
        let response = client
            .get(&me)
            .bearer_auth(&session.token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let identity: serde_json::Value = response.json().await.unwrap();
        assert_eq!(identity["principal"]["projects"], json!([]));
        assert_eq!(
            client
                .get(&me)
                .header("cookie", format!("codifix_session={}", session.token))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            client
                .get(&me)
                .header("x-api-token", &session.token)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let (browser, _) = super::super::mint_web_session(&state, &db.pool, human)
            .await
            .unwrap();
        assert_eq!(
            client
                .get(&me)
                .bearer_auth(&browser)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            client
                .get(format!("{base}/v1/onboarding/company/projects"))
                .bearer_auth(&session.token)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let project = Uuid::new_v4();
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'desktop-test','Desktop','/synthetic')").bind(project).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'reader')").bind(Uuid::new_v4()).bind(human).bind(project).execute(&db.pool).await.unwrap();
        let p = super::super::resolve_principal(
            &state,
            Credential::NativeSession(session.token.clone()),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(p.project_roles.contains_key("desktop-test"));
        sqlx::query("DELETE FROM auth_project_memberships WHERE principal_id=$1")
            .bind(human)
            .execute(&db.pool)
            .await
            .unwrap();
        let p = super::super::resolve_principal(
            &state,
            Credential::NativeSession(session.token.clone()),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(p.project_roles.is_empty());
        assert_eq!(p.credential_source, CredentialSource::NativeSession);
        assert!(p.session_csrf_hash.is_none());
        sqlx::query("UPDATE auth_web_sessions SET expires_at=now()-interval '1 second' WHERE session_hash=$1").bind(hash_secret(&session.token)).execute(&db.pool).await.unwrap();
        assert_eq!(
            client
                .get(&me)
                .bearer_auth(&session.token)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let logout = format!("{base}/v1/auth/desktop/logout");
        for _ in 0..2 {
            let response = client
                .post(&logout)
                .bearer_auth(&session.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert!(!response.headers().contains_key(header::SET_COOKIE));
            assert_eq!(
                response.json::<serde_json::Value>().await.unwrap(),
                json!({"logged_out":true})
            );
        }
        assert_eq!(
            client
                .post(&logout)
                .header("cookie", format!("codifix_session={browser}"))
                .json(&json!({}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            client
                .get(&me)
                .header("cookie", format!("codifix_session={browser}"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        // Only one concurrent exchange may consume a code.
        let mut tx = db.pool.begin().await.unwrap();
        let next_code = issue_desktop_code_tx(&mut tx, human, &context())
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let mut next = request.clone();
        next["code"] = json!(next_code);
        let (left, right) = tokio::join!(
            client.post(&exchange).json(&next).send(),
            client.post(&exchange).json(&next).send()
        );
        let mut statuses = vec![
            left.unwrap().status().as_u16(),
            right.unwrap().status().as_u16(),
        ];
        statuses.sort();
        assert_eq!(statuses, vec![200, 401]);
        let mut tx = db.pool.begin().await.unwrap();
        let expired = issue_desktop_code_tx(&mut tx, human, &context())
            .await
            .unwrap();
        tx.commit().await.unwrap();
        sqlx::query(
            "UPDATE auth_desktop_codes SET expires_at=now()-interval '1 second' WHERE code_hash=$1",
        )
        .bind(hash_secret(&expired))
        .execute(&db.pool)
        .await
        .unwrap();
        let mut next = request.clone();
        next["code"] = json!(expired);
        assert_eq!(
            client
                .post(&exchange)
                .json(&next)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        sqlx::query("UPDATE auth_principals SET disabled_at=now() WHERE id=$1")
            .bind(human)
            .execute(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            client
                .get(&me)
                .header("cookie", format!("codifix_session={browser}"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            client
                .post(&logout)
                .bearer_auth(&session.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        // A validated provider denial consumes state and returns only a stable error.
        let flowstate = super::super::random_secret(32);
        sqlx::query("INSERT INTO auth_oidc_flows(id,state_hash,nonce,pkce_verifier,desktop_context,expires_at) VALUES($1,$2,'nonce','verifier',$3,now()+interval '10 minutes')").bind(Uuid::new_v4()).bind(hash_secret(&flowstate)).bind(serde_json::to_value(context()).unwrap()).execute(&db.pool).await.unwrap();
        let redirects = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let callback = format!("{base}/v1/auth/callback");
        let denial = redirects
            .get(&callback)
            .query(&[
                ("state", flowstate.as_str()),
                ("error", "provider_private_error"),
                ("error_description", "secret-detail"),
            ])
            .send()
            .await
            .unwrap();
        assert_eq!(denial.status(), StatusCode::SEE_OTHER);
        let location = denial.headers()[header::LOCATION].to_str().unwrap();
        assert!(location.contains("error=access_denied"));
        assert!(location.contains(&context().state));
        assert!(!location.contains("private"));
        assert_eq!(
            redirects
                .get(&callback)
                .query(&[("state", flowstate.as_str()), ("error", "denied")])
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        server.abort();
        db.cleanup().await.unwrap();
    }
}
