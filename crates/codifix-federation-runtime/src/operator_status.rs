// SPDX-License-Identifier: AGPL-3.0-or-later
//! Operator-only local readiness, not a public health route. Uses the existing
//! authenticated, signed status contract and current database writer fence.
use crate::{GatewayConfig, ReceiptTrust, Result, RuntimeError};
use codifix_federation::publication::{InstanceId, SpaceId};
use serde::Serialize;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

#[derive(Serialize)]
pub struct LocalGatewayReadiness {
    pub schema_version: u32,
    pub space: SpaceId,
    pub authority: InstanceId,
    pub audience: InstanceId,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    pub readiness_scope: &'static str,
}

fn local_address(listen: SocketAddr) -> Result<SocketAddr> {
    if listen.port() == 0 {
        return Err(RuntimeError::Configuration);
    }
    let ip = match listen.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip if ip.is_loopback() => ip,
        _ => return Err(RuntimeError::Configuration),
    };
    Ok(SocketAddr::new(ip, listen.port()))
}

/// Explicit enrolled audience only. The config's existing credential is read
/// through the protected secret loader; no token is accepted on argv/stdout.
pub async fn check(
    config: &GatewayConfig,
    space: SpaceId,
    authority: InstanceId,
    audience: InstanceId,
) -> Result<LocalGatewayReadiness> {
    let (address, token_file) = selected(config, space, authority, audience)?;
    let token = crate::private_secret(token_file)?;
    probe(
        address,
        &token,
        ReceiptTrust::new(config.receipt_keys.clone())?,
        space,
        authority,
        audience,
    )
    .await
}

pub async fn check_config_path(
    path: &std::path::Path,
    space: SpaceId,
    authority: InstanceId,
    audience: InstanceId,
    service_name: Option<&str>,
) -> Result<LocalGatewayReadiness> {
    if let Some(name) = service_name {
        #[cfg(windows)]
        {
            let reader = codifix_platform_files::windows::ServiceReader::for_named_service(name)
                .map_err(|_| RuntimeError::Configuration)?;
            let config = crate::load_gateway_config_for_service(path, &reader)?;
            let (address, token_file) = selected(&config, space, authority, audience)?;
            let bytes =
                codifix_platform_files::windows::ServiceInput::open(token_file, 4096, &reader)
                    .and_then(|input| input.read(4096))
                    .map_err(|_| RuntimeError::Configuration)?;
            let token = std::str::from_utf8(&bytes)
                .map_err(|_| RuntimeError::Configuration)?
                .trim();
            if !(32..=4096).contains(&token.len()) {
                return Err(RuntimeError::Configuration);
            }
            return probe(
                address,
                token,
                ReceiptTrust::new(config.receipt_keys.clone())?,
                space,
                authority,
                audience,
            )
            .await;
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            return Err(RuntimeError::Configuration);
        }
    }
    check(
        &crate::load_gateway_config(path)?,
        space,
        authority,
        audience,
    )
    .await
}

fn selected(
    config: &GatewayConfig,
    space: SpaceId,
    authority: InstanceId,
    audience: InstanceId,
) -> Result<(SocketAddr, &std::path::Path)> {
    if config.space != space || config.authority != authority {
        return Err(RuntimeError::Configuration);
    }
    let address = local_address(config.listen)?;
    let matching: Vec<_> = config
        .credentials
        .iter()
        .filter(|credential| credential.instance == audience)
        .collect();
    if matching.len() != 1 {
        return Err(RuntimeError::Configuration);
    }
    Ok((address, &matching[0].token_file))
}

async fn probe(
    address: SocketAddr,
    token: &str,
    trust: ReceiptTrust,
    space: SpaceId,
    authority: InstanceId,
    audience: InstanceId,
) -> Result<LocalGatewayReadiness> {
    // Do not loosen general Endpoint HTTPS requirements. This distinct operator
    // client can contact only a configured numeric loopback socket, never DNS.
    if !address.ip().is_loopback() || address.port() == 0 {
        return Err(RuntimeError::Configuration);
    }
    let nonce = uuid::Uuid::new_v4();
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|_| RuntimeError::Configuration)?;
    let response = client
        .get(format!("http://{address}/v1/status"))
        .bearer_auth(token)
        .query(&[("nonce", nonce)])
        .send()
        .await
        .map_err(|_| RuntimeError::Unavailable)?;
    crate::sync::check_status(response.status())?;
    let signed = crate::http::json(response, 16 * 1024).await?;
    // A valid response requires current Pull membership and writer-fence checks
    // in the gateway, plus exact nonce/audience/space/authority/key/freshness.
    trust.verify_status(signed, nonce, audience, space, authority)?;
    Ok(LocalGatewayReadiness {
        schema_version: 1,
        space,
        authority,
        audience,
        observed_at: chrono::Utc::now(),
        readiness_scope: "signed_identity_current_pull_and_writer_fence_only",
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn local_readiness_never_resolves_or_contacts_nonloopback_addresses() {
        assert_eq!(
            local_address("0.0.0.0:4567".parse().unwrap())
                .unwrap()
                .to_string(),
            "127.0.0.1:4567"
        );
        assert_eq!(
            local_address("[::]:4567".parse().unwrap())
                .unwrap()
                .to_string(),
            "[::1]:4567"
        );
        assert!(local_address("192.0.2.1:4567".parse().unwrap()).is_err());
        assert!(local_address("127.0.0.1:0".parse().unwrap()).is_err());
    }
    #[tokio::test]
    async fn local_probe_requires_authenticated_fresh_exact_signed_status() {
        use axum::{Json, Router, extract::Query, http::HeaderMap, routing::get};
        use std::sync::Arc;
        let security = Arc::new(
            crate::GatewaySecurity::initial(
                &[42; 32],
                codifix_federation_store::SpacePolicy::default(),
            )
            .unwrap(),
        );
        let trust = security.trust();
        let space = SpaceId::new(uuid::Uuid::new_v4()).unwrap();
        let authority = InstanceId::new(uuid::Uuid::new_v4()).unwrap();
        let audience = InstanceId::new(uuid::Uuid::new_v4()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route("/v1/status", get(move |headers: HeaderMap, Query(query): Query<std::collections::HashMap<String,String>>| {
            let security = security.clone(); async move {
                assert_eq!(headers.get("authorization").unwrap(), "Bearer synthetic-operator-token-0123456789");
                let nonce = uuid::Uuid::parse_str(query.get("nonce").unwrap()).unwrap();
                Json(security.sign_status(crate::GatewayStatus { credentials: crate::GatewayCredentialStatus::unknown(crate::credentials::now_seconds()), pending: None }, nonce, audience, space, authority).unwrap())
            }
        }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let ready = probe(
            address,
            "synthetic-operator-token-0123456789",
            trust.clone(),
            space,
            authority,
            audience,
        )
        .await
        .unwrap();
        let output = serde_json::to_string(&ready).unwrap();
        assert!(output.contains("writer_fence_only"));
        assert!(!output.contains("token"));
        assert!(!output.contains("credentials"));
        assert!(
            probe(
                address,
                "synthetic-operator-token-0123456789",
                trust,
                space,
                InstanceId::new(uuid::Uuid::new_v4()).unwrap(),
                audience
            )
            .await
            .is_err()
        );
        server.abort();
    }
}
