// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded read-only observations use the ordinary project service identity.
use super::ApiClient;
use anyhow::{Result, ensure};
use codifix_record::agency_observation::{CompanionStatus, ExchangeDetail, ExchangePage};
use reqwest::{Client, Url};
use uuid::Uuid;

fn observation_path(project: &str, id: Option<Uuid>, companion: bool) -> Result<Url> {
    let mut url = Url::parse("http://localhost/")?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Invalid observation path"))?;
        path.extend([
            "v1",
            "projects",
            project,
            "agency",
            if companion { "companion" } else { "exchanges" },
        ]);
        if let Some(id) = id {
            path.push(&id.to_string());
        }
    }
    Ok(url)
}
impl ApiClient {
    pub(crate) async fn companion_status(&self, project: &str) -> Result<CompanionStatus> {
        self.observation(observation_path(project, None, true)?)
            .await
    }
    pub(crate) async fn agent_exchanges(
        &self,
        project: &str,
        peer: Option<&str>,
        before: Option<&str>,
        limit: u16,
    ) -> Result<ExchangePage> {
        ensure!(
            (1..=100).contains(&limit),
            "Exchange page limit must be 1 to 100"
        );
        ensure!(
            peer.is_none_or(|p| p.len() <= 256) && before.is_none_or(|p| p.len() <= 512),
            "Exchange filter exceeds size limit"
        );
        let mut url = observation_path(project, None, false)?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("limit", &limit.to_string());
            if let Some(peer) = peer {
                query.append_pair("peer", peer);
            }
            if let Some(before) = before {
                query.append_pair("before", before);
            }
        }
        self.observation(url).await
    }
    pub(crate) async fn agent_exchange(&self, project: &str, id: Uuid) -> Result<ExchangeDetail> {
        self.observation(observation_path(project, Some(id), false)?)
            .await
    }
    async fn observation<T: serde::de::DeserializeOwned>(&self, path: Url) -> Result<T> {
        let endpoint = crate::commands::output::service_url(&self.config, path.path());
        let mut endpoint = Url::parse(&endpoint)?;
        endpoint.set_query(path.query());
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(self.config.service.request_timeout)
            .build()?;
        let mut response = client
            .get(endpoint)
            .headers(crate::commands::output::write_headers(&self.config)?)
            .header("cache-control", "no-store")
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Observation service connection failed"))?;
        let status = response.status();
        ensure!(
            status.is_success(),
            "Observation returned HTTP {}",
            status.as_u16()
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("Observation response interrupted"))?
        {
            ensure!(
                bytes.len() + chunk.len() <= 1024 * 1024,
                "Observation response exceeds size limit"
            );
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("Invalid observation response"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_and_task_are_path_segments() {
        let id = Uuid::nil();
        let path = observation_path("a/b?#", Some(id), false).unwrap();
        assert_eq!(
            path.path(),
            format!("/v1/projects/a%2Fb%3F%23/agency/exchanges/{id}")
        );
        assert!(path.query().is_none());
        assert_eq!(
            observation_path("demo", None, true).unwrap().path(),
            "/v1/projects/demo/agency/companion"
        );
    }
}
