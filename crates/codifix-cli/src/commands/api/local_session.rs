// SPDX-License-Identifier: AGPL-3.0-or-later
//! Revocable loopback owner sessions. Credentials remain in memory only.
use super::ApiClient;
use crate::commands::output::{service_url, write_headers};
use anyhow::{Result, bail, ensure};
use codifix_config::AppConfig;
use codifix_record::{AuthMeResponse, AuthMode, AuthPrincipalKind, AuthRole};
use reqwest::{
    Client, Url,
    header::{COOKIE, HeaderMap, ORIGIN, SET_COOKIE},
};

fn client() -> Result<Client> {
    Ok(Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}

/// Explicit local-human transport: never falls back to a token or remints a
/// rejected session. The caller must keep requests on its configured local API.
pub(in crate::commands) async fn local_human_client(config: AppConfig) -> Result<Client> {
    let api = ApiClient::new(client()?, config);
    let headers = api.local_session_headers(&api.client, true).await?;
    Ok(Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .default_headers(headers)
        .build()?)
}

impl ApiClient {
    pub(super) async fn local_session_headers(
        &self,
        client: &Client,
        required: bool,
    ) -> Result<HeaderMap> {
        if required {
            ensure!(
                self.config
                    .service
                    .bind_addr
                    .parse::<std::net::SocketAddr>()
                    .is_ok_and(|address| address.ip().is_loopback()),
                "Local-human session requires a literal loopback service address"
            );
        }
        let base = Url::parse(&service_url(&self.config, "/"))?;
        // Do not resolve a hostname or trust a redirect/proxy with owner authority.
        let loopback = base
            .host_str()
            .and_then(|host| {
                host.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .ok()
            })
            .is_some_and(|ip| ip.is_loopback());
        if !loopback {
            ensure!(
                !required,
                "Local-human session requires a literal loopback service address"
            );
            return write_headers(&self.config);
        }
        let mut session = self.question_session.lock().await;
        if let Some(headers) = &*session {
            return Ok(headers.clone());
        }
        let response = client
            .get(service_url(&self.config, "/v1/auth/me"))
            .headers(write_headers(&self.config)?)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Could not inspect local authentication"))?;
        ensure!(
            response.status().is_success(),
            "Local authentication unavailable"
        );
        let me: AuthMeResponse = response
            .json()
            .await
            .map_err(|_| anyhow::anyhow!("Invalid authentication response"))?;
        if me.mode != AuthMode::SingleUser || me.principal.kind != AuthPrincipalKind::Internal {
            ensure!(
                !required,
                "Local-human session requires the configured single-user owner credential"
            );
            return write_headers(&self.config);
        }
        ensure!(
            me.authenticated
                && me.principal.global_role == Some(AuthRole::Admin)
                && me.principal.id == codifix_record::LOCAL_OWNER_PRINCIPAL_ID,
            "Local owner authentication required; no credentials were changed"
        );
        let origin = base.origin().ascii_serialization();
        let response = client
            .post(service_url(&self.config, "/v1/auth/session/bootstrap"))
            .header(ORIGIN, &origin)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Local session bootstrap unconfirmed"))?;
        ensure!(
            response.status().is_success(),
            "Local session bootstrap denied"
        );
        let mut auth = None;
        let mut csrf = None;
        for header in response.headers().get_all(SET_COOKIE) {
            let value = header
                .to_str()
                .map_err(|_| anyhow::anyhow!("Invalid local session response"))?;
            if let Some((name, value)) = value.split(';').next().and_then(|v| v.split_once('=')) {
                let slot = match name {
                    "memory_session" => &mut auth,
                    "memory_csrf" => &mut csrf,
                    _ => continue,
                };
                ensure!(slot.is_none(), "Duplicate local session cookie");
                ensure!(
                    !value.is_empty()
                        && value
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
                    "Invalid local session cookie"
                );
                *slot = Some(value.to_owned());
            }
        }
        let (Some(auth), Some(csrf)) = (auth, csrf) else {
            bail!("Incomplete local session response");
        };
        let mut cookie: reqwest::header::HeaderValue =
            format!("memory_session={auth}; memory_csrf={csrf}")
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid session cookie"))?;
        cookie.set_sensitive(true);
        let mut csrf: reqwest::header::HeaderValue = csrf
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid session token"))?;
        csrf.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, cookie);
        headers.insert("x-csrf-token", csrf);
        headers.insert(ORIGIN, origin.parse()?);
        *session = Some(headers.clone());
        Ok(headers)
    }
}
