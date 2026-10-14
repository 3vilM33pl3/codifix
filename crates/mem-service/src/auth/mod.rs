// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) mod answerer;
pub(crate) mod company_agents;
pub(crate) mod company_catalog;
pub(crate) mod company_delegate;
pub(crate) mod company_delivery_delegate;
pub(crate) mod company_device_delivery;
mod company_device_proxy;
pub(crate) mod company_devices;
pub(crate) mod device;
pub(crate) mod device_contributions;
pub(crate) mod device_replication;
mod model;
mod oidc;
pub(crate) mod onboarding;
mod policy;
pub(crate) mod scoped_delivery;
#[cfg(test)]
mod scoped_token_tests;
mod store;

pub(crate) use model::*;
pub(crate) use oidc::*;
pub(crate) use policy::*;
pub(crate) use store::*;

use axum::http::HeaderMap;

use crate::ApiError;

pub(crate) fn require_strict_token(headers: &HeaderMap, expected: &str) -> Result<(), ApiError> {
    let Some(provided) = headers
        .get("x-api-token")
        .and_then(|value| value.to_str().ok())
    else {
        return Err(ApiError::unauthorized("missing x-api-token header"));
    };
    if provided != expected {
        return Err(ApiError::unauthorized("invalid api token"));
    }
    Ok(())
}
