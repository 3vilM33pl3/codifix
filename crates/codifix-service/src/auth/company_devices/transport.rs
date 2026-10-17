// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use serde::de::DeserializeOwned;
use std::time::Duration;
fn unavailable() -> ApiError {
    ApiError::service_unavailable(
        "current owner confirmation unavailable; no current authority asserted",
    )
}

pub(super) fn validate_receipt(
    request: &OwnerDeviceApprovalRequest,
    receipt: &OwnerDeviceApprovalReceipt,
) -> Result<(), ApiError> {
    if receipt.request != *request
        || receipt.request_sha256 != company_catalog::digest(request)?
        || receipt.human_principal_id.is_nil()
        || receipt.committed_at > Utc::now() + chrono::Duration::minutes(5)
    {
        return Err(denied());
    }
    Ok(())
}
pub(super) fn validate_bootstrap(
    request: &OwnerDeviceApprovalRequest,
    bootstrap: &DeviceProjectSubscription,
) -> Result<(), ApiError> {
    if bootstrap.device != request.device_id
        || bootstrap.scope.company != request.company_id
        || bootstrap.scope.area != request.area.as_str()
        || bootstrap.scope.owner.as_uuid() != request.owner_instance_id
        || bootstrap.scope.project != request.project_id
        || bootstrap.catalog_revision
            != u64::try_from(request.manifest_revision).map_err(|_| denied())?
        || bootstrap.scope.space.as_uuid().is_nil()
        || bootstrap.authority.as_uuid().is_nil()
        || bootstrap.validate().is_err()
    {
        return Err(denied());
    }
    Ok(())
}
async fn post<T: Serialize, R: DeserializeOwned>(
    owner: &codifix_config::CompanyOwnerConfig,
    path: &str,
    body: &T,
) -> Result<R, ApiError> {
    let mut url = company_catalog::origin(&owner.base_url)?;
    url.set_path(path);
    let env = owner.delivery_token_env.as_ref().ok_or_else(denied)?;
    if env == &owner.token_env {
        return Err(denied());
    }
    let secret = std::env::var(env).map_err(|_| denied())?;
    if !(32..=4096).contains(&secret.len())
        || secret.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(denied());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| unavailable())?;
    let mut response = client
        .post(url)
        .bearer_auth(secret)
        .json(body)
        .send()
        .await
        .map_err(|_| unavailable())?;
    if !response.status().is_success() {
        // Never reflect the owner body. A definitive denial must reach the
        // native broker so it can latch revocation before another offline call.
        return Err(match response.status().as_u16() {
            401 | 403 | 404 | 410 => denied(),
            409 => changed(),
            _ => unavailable(),
        });
    }
    if response.content_length().is_some_and(|n| n > 65536) {
        return Err(denied());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len() + chunk.len() > 65536 {
            return Err(denied());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| denied())
}
async fn outcome(
    state: &AppState,
    binding: &Binding,
    receipt: Option<&OwnerDeviceApprovalReceipt>,
    bootstrap: Option<&DeviceProjectSubscription>,
) -> Result<(), ApiError> {
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let row=sqlx::query("SELECT receipt_json,bootstrap_json FROM onboarding_company_device_outcomes WHERE operation_id=$1 AND request_id=$2 FOR UPDATE").bind(binding.request.operation_id).bind(binding.request.device_request_id).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
    let old_receipt: Option<SqlJson<OwnerDeviceApprovalReceipt>> =
        row.try_get("receipt_json").map_err(ApiError::sql)?;
    let old_bootstrap: Option<SqlJson<DeviceProjectSubscription>> =
        row.try_get("bootstrap_json").map_err(ApiError::sql)?;
    if receipt
        .zip(old_receipt.as_ref())
        .is_some_and(|(new, old)| new != &old.0)
        || bootstrap
            .zip(old_bootstrap.as_ref())
            .is_some_and(|(new, old)| new != &old.0)
    {
        return Err(changed());
    }
    let receipt = receipt.cloned().or_else(|| old_receipt.map(|r| r.0));
    let bootstrap = bootstrap.cloned().or_else(|| old_bootstrap.map(|b| b.0));
    let confirmed = receipt.is_some() && bootstrap.is_some();
    sqlx::query("UPDATE onboarding_company_device_outcomes SET receipt_json=$2,bootstrap_json=$3,state=$4,updated_at=clock_timestamp() WHERE operation_id=$1").bind(binding.request.operation_id).bind(receipt.map(SqlJson)).bind(bootstrap.map(SqlJson)).bind(if confirmed {"confirmed"} else {"pending"}).execute(&mut *tx).await.map_err(ApiError::sql)?;
    tx.commit().await.map_err(ApiError::sql)
}
pub(super) async fn confirm(state: &AppState, binding: &Binding) -> Result<(), ApiError> {
    let owner = company_catalog::delivery_owner(
        state,
        binding.request.area.as_str(),
        binding.request.owner_instance_id,
    )?;
    let row=sqlx::query("SELECT state,receipt_json,bootstrap_json FROM onboarding_company_device_outcomes WHERE operation_id=$1 AND request_id=$2").bind(binding.request.operation_id).bind(binding.request.device_request_id).fetch_one(&state.pool()?).await.map_err(ApiError::sql)?;
    if row.try_get::<String, _>("state").map_err(ApiError::sql)? == "confirmed" {
        let receipt = row
            .try_get::<SqlJson<OwnerDeviceApprovalReceipt>, _>("receipt_json")
            .map_err(ApiError::sql)?
            .0;
        let bootstrap = row
            .try_get::<SqlJson<DeviceProjectSubscription>, _>("bootstrap_json")
            .map_err(ApiError::sql)?
            .0;
        validate_receipt(&binding.request, &receipt)?;
        validate_bootstrap(&binding.request, &bootstrap)?;
        return Ok(());
    }
    let result = async {
        // Exact stable operation UUID replays the owner journal after a lost reply.
        let receipt: OwnerDeviceApprovalReceipt = post(
            &owner,
            "/v1/onboarding/company-delivery/approve",
            &binding.request,
        )
        .await?;
        validate_receipt(&binding.request, &receipt)?;
        outcome(state, binding, Some(&receipt), None).await?;
        let bootstrap: DeviceProjectSubscription = post(
            &owner,
            "/v1/onboarding/company-delivery/subscription",
            &OwnerDeviceSubscriptionRequest {
                approval_id: binding.request.operation_id,
            },
        )
        .await?;
        validate_bootstrap(&binding.request, &bootstrap)?;
        outcome(state, binding, Some(&receipt), Some(&bootstrap)).await
    }
    .await;
    if result.is_err() {
        // Failure text never contains an owner response, credential or SQL detail.
        let _=sqlx::query("UPDATE onboarding_company_device_outcomes SET state='uncertain',updated_at=clock_timestamp() WHERE operation_id=$1 AND state<>'confirmed'").bind(binding.request.operation_id).execute(&state.pool()?).await;
        return Err(uncertain(binding.request.device_request_id));
    }
    Ok(())
}

/// Current owner authority, not a historical approval receipt. Trust remains
/// pinned to the saved authenticated bootstrap and is never replaced here.
pub(super) async fn current_bootstrap(
    owner: &codifix_config::CompanyOwnerConfig,
    approval: &OwnerDeviceApprovalReceipt,
    saved: &DeviceProjectSubscription,
) -> Result<(), ApiError> {
    let current: DeviceProjectSubscription = post(
        owner,
        "/v1/onboarding/company-delivery/subscription",
        &OwnerDeviceSubscriptionRequest {
            approval_id: approval.request.operation_id,
        },
    )
    .await?;
    validate_bootstrap(&approval.request, &current)?;
    if current != *saved {
        return Err(changed());
    }
    Ok(())
}
