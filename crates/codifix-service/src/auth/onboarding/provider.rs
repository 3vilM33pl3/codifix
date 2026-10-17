// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded, opt-in Authentik adapter. Never assumes IdP and SQL are atomic.
use super::*;
use sha2::{Digest, Sha256};
use std::time::Duration;

fn unavailable() -> ApiError {
    ApiError::service_unavailable(
        "Authentik management is unavailable; verify the restricted server configuration",
    )
}

async fn bounded_json(mut response: reqwest::Response) -> Option<Value> {
    let mut bytes = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if bytes.len() + chunk.len() <= 16 * 1024 => {
                bytes.extend_from_slice(&chunk)
            }
            Ok(None) => return serde_json::from_slice(&bytes).ok(),
            _ => return None,
        }
    }
}

pub(super) async fn operate(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
    operation: &str,
    input: Option<OnboardingIdpUserRequest>,
    provider_id: Option<i64>,
) -> Result<OnboardingIdpOperation, ApiError> {
    let cfg = &state.config.auth.authentik_management;
    if !cfg.enabled
        || !safe_https(codifix_config::oidc_principal_namespace(
            &state.config.auth.oidc.issuer_url,
        ))
        || cfg.token_env.is_empty()
        || cfg.user_path.trim().is_empty()
        || cfg.user_path.len() > 256
        || cfg.user_path.chars().any(char::is_control)
        || cfg.invitation_flow.is_some_and(|id| id.is_nil())
    {
        return Err(unavailable());
    }
    let mut base = reqwest::Url::parse(&cfg.base_url).map_err(|_| unavailable())?;
    // Plain HTTP is compiled only for fake loopback provider tests, never accepted by a released service.
    let fake_loopback = cfg!(test)
        && base.scheme() == "http"
        && base
            .host_str()
            .is_some_and(|h| h == "127.0.0.1" || h == "localhost");
    if !(safe_https(&cfg.base_url) || fake_loopback)
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
        || base.path() != "/"
    {
        return Err(unavailable());
    }
    let token = std::env::var(&cfg.token_env).map_err(|_| unavailable())?;
    if !text(&token, 4096) {
        return Err(unavailable());
    }
    if let Some(input) = &input
        && (!text(&input.username, 150)
            || !input
                .username
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'@'))
            || !text(&input.display_name, 256)
            || !text(&input.email, 320)
            || !input.email.contains('@'))
    {
        return Err(bad("bounded username, display name and email are required"));
    }
    let id = Uuid::new_v4();
    let provider_origin = base.origin().ascii_serialization();
    let provider_correlation = if operation == "invite" {
        format!("memory-onboarding-{}", id.simple())
    } else {
        id.to_string()
    };
    let (method, path, body) = match (operation, input) {
        ("create", Some(input)) => (
            reqwest::Method::POST,
            "/api/v3/core/users/".to_string(),
            json!({"username":input.username,"name":input.display_name,"email":input.email,"is_active":true,"path":cfg.user_path,"type":"internal","groups":[],"attributes":{"memory_onboarding_operation":provider_correlation}}),
        ),
        ("invite", Some(input)) => {
            let flow = cfg.invitation_flow.ok_or_else(|| {
                ApiError::service_unavailable("Authentik invitation flow is not configured")
            })?;
            (
                reqwest::Method::POST,
                "/api/v3/stages/invitation/invitations/".to_string(),
                json!({"name":provider_correlation,"expires":chrono::Utc::now()+chrono::Duration::hours(24),"single_use":true,"flow":flow,"fixed_data":{"username":input.username,"name":input.display_name,"email":input.email}}),
            )
        }
        ("disable", None) => (
            reqwest::Method::PATCH,
            format!(
                "/api/v3/core/users/{}/",
                provider_id.ok_or_else(|| bad("provider ID required"))?
            ),
            json!({"is_active":false}),
        ),
        _ => return Err(bad("unsupported provider operation")),
    };
    base.set_path(&path);
    let request_sha256=format!("{:x}",Sha256::digest(serde_json::to_vec(&json!({"operation":operation,"provider_origin":provider_origin,"path":path,"provider_user_id":provider_id,"managed_user_path":cfg.user_path,"body":body})).map_err(|_|unavailable())?));
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(3))
        .build()
        .map_err(|_| unavailable())?;
    let request = client
        .request(method, base.clone())
        .bearer_auth(&token)
        .json(&body)
        .build()
        .map_err(|_| unavailable())?;
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    lock_global_administrator(state, &mut tx, actor).await?;
    if operation == "disable" {
        // Server-pinned managed path limits the adapter's scope even if the
        // operator mistakenly supplied an unrelated privileged provider ID.
        let response = client
            .get(base)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|_| unavailable())?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
        let target = bounded_json(response).await.ok_or_else(unavailable)?;
        if target.get("pk").and_then(Value::as_i64) != provider_id
            || target.get("path").and_then(Value::as_str) != Some(cfg.user_path.as_str())
            || target.get("is_superuser").and_then(Value::as_bool) != Some(false)
        {
            return Err(ApiError::forbidden(
                "provider user is outside the managed onboarding path",
            ));
        }
    }
    sqlx::query("INSERT INTO onboarding_idp_intents(id,actor_principal_id,operation,provider_user_id,provider_origin,provider_correlation,request_sha256) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(id).bind(actor.id).bind(operation).bind(provider_id).bind(&provider_origin).bind(&provider_correlation).bind(&request_sha256).execute(&mut *tx).await.map_err(ApiError::sql)?;
    audit(
        &mut tx,
        actor.id,
        None,
        "onboarding.idp.intent",
        json!({"operation_id":id,"operation":operation}),
    )
    .await?;
    tx.commit().await.map_err(ApiError::sql)?;
    // Recheck after the durable intent commit, and keep credential/principal
    // locks through the one external send. A concurrent disable/revoke waits
    // for this bounded attempt; no cached administrator can start a new one.
    let mut execution_tx = pool.begin().await.map_err(ApiError::sql)?;
    if lock_global_administrator(state, &mut execution_tx, actor)
        .await
        .is_err()
    {
        execution_tx.rollback().await.map_err(ApiError::sql)?;
        let mut denied_tx = pool.begin().await.map_err(ApiError::sql)?;
        sqlx::query("INSERT INTO onboarding_idp_outcomes(intent_id,outcome,provider_user_id) VALUES($1,'error',$2)")
            .bind(id).bind(provider_id).execute(&mut *denied_tx).await.map_err(ApiError::sql)?;
        audit(
            &mut denied_tx,
            actor.id,
            None,
            "onboarding.idp.not_sent",
            json!({"operation_id":id}),
        )
        .await?;
        denied_tx.commit().await.map_err(ApiError::sql)?;
        return Err(ApiError::forbidden(&format!(
            "provider operation {id} was not sent; current global administration is required"
        )));
    }
    // No automatic retry: a timeout after provider acceptance is uncertain.
    let (outcome, returned_id, invitation_id) = match client.execute(request).await {
        Err(_) => ("uncertain", provider_id, None),
        Ok(response) if response.status().is_success() => {
            let value = bounded_json(response).await;
            let returned = value
                .as_ref()
                .and_then(|v| v.get("pk"))
                .and_then(Value::as_i64)
                .filter(|v| *v > 0);
            let invitation = value
                .as_ref()
                .and_then(|v| v.get("pk"))
                .and_then(Value::as_str)
                .and_then(|v| Uuid::parse_str(v).ok());
            let valid = match operation {
                "create" => returned.is_some(),
                "invite" => invitation.is_some(),
                "disable" => returned == provider_id,
                _ => false,
            };
            (
                if valid { "success" } else { "uncertain" },
                returned.or(provider_id),
                invitation,
            )
        }
        // A 5xx can occur after an effect. No body, request details, or token is retained.
        Ok(response) => (
            if response.status().is_server_error() {
                "uncertain"
            } else {
                "error"
            },
            provider_id,
            None,
        ),
    };
    let recorded=async {
        sqlx::query("INSERT INTO onboarding_idp_outcomes(intent_id,outcome,provider_user_id,invitation_id) VALUES($1,$2,$3,$4)")
        .bind(id).bind(outcome).bind(returned_id).bind(invitation_id).execute(&mut *execution_tx).await?;
    sqlx::query("INSERT INTO auth_audit_events(id,actor_principal_id,event_type,outcome,metadata_json) VALUES($1,$2,'onboarding.idp.outcome',$3,$4)")
        .bind(Uuid::new_v4()).bind(actor.id).bind(if outcome=="success" {"success"} else {"error"})
        .bind(json!({"operation_id":id,"operation":operation,"provider_outcome":outcome})).execute(&mut *execution_tx).await?;
        execution_tx.commit().await
    }.await;
    recorded.map_err(|_:sqlx::Error|ApiError::service_unavailable(&format!("external effect may have occurred; inspect provider operation {id}; outcome could not be recorded")))?;
    Ok(OnboardingIdpOperation {
        id,
        operation: operation.into(),
        outcome: outcome.into(),
        provider_origin: Some(provider_origin),
        provider_correlation: Some(provider_correlation),
        request_sha256: Some(request_sha256),
        provider_user_id: returned_id,
        invitation_id,
    })
}
