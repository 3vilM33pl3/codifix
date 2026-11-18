// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded read-only observations use the ordinary project service identity.
use std::collections::BTreeMap;

use super::ApiClient;
use anyhow::{Result, ensure};
use codifix_record::agency_observation::{
    CompanionStatus, ExchangeDetail, ExchangePage, ExchangeSummary,
};
use futures_util::{StreamExt, TryStreamExt, stream};
use reqwest::{Client, Url};
use uuid::Uuid;

const TRANSCRIPT_PAGE_SIZE: u16 = 25;
const TRANSCRIPT_DETAIL_CONCURRENCY: usize = 4;

pub(crate) struct TranscriptBatch {
    pub page: ExchangePage,
    pub details: Vec<ExchangeDetail>,
}

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

    pub(crate) async fn agent_transcript_page(
        &self,
        project: &str,
        peer: Option<&str>,
        before: Option<&str>,
        known: &BTreeMap<Uuid, ExchangeSummary>,
    ) -> Result<TranscriptBatch> {
        let page = self
            .agent_exchanges(project, peer, before, TRANSCRIPT_PAGE_SIZE)
            .await?;
        ensure!(
            page.exchanges.len() <= usize::from(TRANSCRIPT_PAGE_SIZE),
            "Observation exchange page exceeds size limit"
        );
        let changed = page
            .exchanges
            .iter()
            .filter(|exchange| known.get(&exchange.task_id) != Some(*exchange))
            .map(|exchange| {
                (
                    exchange.task_id,
                    exchange.peer.clone(),
                    exchange.direction.clone(),
                )
            })
            .collect::<Vec<_>>();
        let details = stream::iter(changed)
            .map(|(task_id, peer, direction)| async move {
                let detail = self.agent_exchange(project, task_id).await?;
                ensure!(
                    detail.exchange.task_id == task_id
                        && detail.exchange.peer == peer
                        && detail.exchange.direction == direction,
                    "Observation exchange detail does not match its expected scope"
                );
                Ok(detail)
            })
            .buffer_unordered(TRANSCRIPT_DETAIL_CONCURRENCY)
            .try_collect()
            .await?;
        Ok(TranscriptBatch { page, details })
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
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    async fn observation_client() -> (ApiClient, TcpListener) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let config = serde_json::from_value(serde_json::json!({
            "service": {
                "bind_addr": listener.local_addr().unwrap().to_string(),
                "api_token": "synthetic"
            },
            "database": {"url": "postgresql://unused/unused"}
        }))
        .unwrap();
        (ApiClient::new(Client::new(), config), listener)
    }

    async fn serve(
        listener: TcpListener,
        responses: impl IntoIterator<Item = (u16, String)>,
    ) -> Vec<String> {
        let mut paths = Vec::new();
        for (status, body) in responses {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut line = String::new();
            socket.read_line(&mut line).await.unwrap();
            paths.push(line.split_whitespace().nth(1).unwrap().to_owned());
            loop {
                line.clear();
                let count = socket.read_line(&mut line).await.unwrap();
                assert!(count > 0);
                if line == "\r\n" {
                    break;
                }
            }
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
        paths
    }

    fn summary(task_id: Uuid) -> ExchangeSummary {
        let at = "2026-11-18T00:00:00Z".parse().unwrap();
        ExchangeSummary {
            task_id,
            peer: "peer".into(),
            direction: "outgoing".into(),
            specialist_id: "test".into(),
            objective: "Test objective".into(),
            status: "active".into(),
            created_at: at,
            updated_at: at,
            journal_available: true,
        }
    }

    fn detail(exchange: ExchangeSummary) -> ExchangeDetail {
        ExchangeDetail {
            remote_task_id: None,
            exchange,
            events: vec![],
            events_truncated: false,
            independently_verified: false,
        }
    }

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

    #[tokio::test]
    async fn transcript_page_reuses_complete_summaries_and_accepts_a_newer_detail() {
        let (api, listener) = observation_client().await;
        let cached = summary(Uuid::new_v4());
        let initial = summary(Uuid::new_v4());
        let mut changed = initial.clone();
        changed.objective = "Updated objective with the same timestamp".into();
        let page = ExchangePage {
            exchanges: vec![cached.clone(), changed.clone()],
            next_before: Some("next cursor".into()),
        };
        let mut newer = changed;
        newer.status = "completed".into();
        newer.updated_at = "2026-11-18T00:00:01Z".parse().unwrap();
        let detail = detail(newer);
        let server = tokio::spawn(serve(
            listener,
            [
                (200, serde_json::to_string(&page).unwrap()),
                (200, serde_json::to_string(&detail).unwrap()),
            ],
        ));

        let known = BTreeMap::from([(cached.task_id, cached), (initial.task_id, initial)]);
        let batch = tokio::spawn(async move {
            api.agent_transcript_page("demo", Some("peer"), Some("prior cursor"), &known)
                .await
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(batch.page, page);
        assert_eq!(batch.details, vec![detail.clone()]);
        assert_eq!(
            server.await.unwrap(),
            [
                "/v1/projects/demo/agency/exchanges?limit=25&peer=peer&before=prior+cursor".into(),
                format!(
                    "/v1/projects/demo/agency/exchanges/{}",
                    detail.exchange.task_id
                ),
            ]
        );
    }

    #[tokio::test]
    async fn transcript_detail_rejects_scope_mismatches_and_preserves_http_errors() {
        let (api, listener) = observation_client().await;
        let expected = summary(Uuid::new_v4());
        let mut wrong_task = expected.clone();
        wrong_task.task_id = Uuid::new_v4();
        let mut wrong_peer = expected.clone();
        wrong_peer.peer = "other".into();
        let mut wrong_direction = expected.clone();
        wrong_direction.direction = "incoming".into();
        let page = serde_json::to_string(&ExchangePage {
            exchanges: vec![expected.clone()],
            next_before: None,
        })
        .unwrap();
        let responses = [wrong_task, wrong_peer, wrong_direction]
            .map(|exchange| (200, serde_json::to_string(&detail(exchange)).unwrap()))
            .into_iter()
            .chain([401, 403, 404].map(|status| (status, "private error body".into())))
            .flat_map(|detail_response| [(200, page.clone()), detail_response])
            .collect::<Vec<_>>();
        let server = tokio::spawn(serve(listener, responses));

        for expected_error in [
            "Observation exchange detail does not match its expected scope",
            "Observation exchange detail does not match its expected scope",
            "Observation exchange detail does not match its expected scope",
            "Observation returned HTTP 401",
            "Observation returned HTTP 403",
            "Observation returned HTTP 404",
        ] {
            let Err(error) = api
                .agent_transcript_page("demo", None, None, &BTreeMap::new())
                .await
            else {
                panic!("batch returned invalid detail");
            };
            assert_eq!(error.to_string(), expected_error);
        }
        assert_eq!(
            server.await.unwrap(),
            [
                "/v1/projects/demo/agency/exchanges?limit=25".to_owned(),
                format!("/v1/projects/demo/agency/exchanges/{}", expected.task_id),
            ]
            .into_iter()
            .cycle()
            .take(12)
            .collect::<Vec<_>>()
        );
    }
}
