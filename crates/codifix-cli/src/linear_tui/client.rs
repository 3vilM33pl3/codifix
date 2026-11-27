// SPDX-License-Identifier: AGPL-3.0-or-later
use anyhow::{Context, Result, ensure};
use chrono::{Duration, Utc};
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{net::IpAddr, path::Path};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    endpoint: String,
    peer: String,
    token_env: String,
    local_specialist: String,
    local_mandate_id: Uuid,
    remote_peer: String,
    remote_specialist: String,
    remote_mandate_id: Uuid,
    linear_project_id: Uuid,
}

pub(super) struct Agency {
    profile: Profile,
    client: Client,
    token: String,
    origin: Url,
}

impl Agency {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let p: Profile =
            serde_json::from_slice(&std::fs::read(path).context("read Linear profile")?)
                .context("parse Linear profile")?;
        let origin = Url::parse(&p.endpoint)?;
        let ip = origin
            .host_str()
            .unwrap_or_default()
            .trim_matches(['[', ']'])
            .parse::<IpAddr>();
        ensure!(
            ip.is_ok_and(|ip| ip.is_loopback())
                && matches!(origin.scheme(), "http" | "https")
                && origin.path() == "/"
                && origin.query().is_none()
                && origin.fragment().is_none()
                && origin.username().is_empty()
                && origin.password().is_none(),
            "Linear profile requires a loopback HTTP origin"
        );
        let token = std::env::var(&p.token_env).context("Linear operator token unavailable")?;
        ensure!(
            !token.is_empty() && !p.peer.trim().is_empty(),
            "Linear operator profile is incomplete"
        );
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(8))
            .build()?;
        Ok(Self {
            profile: p,
            token,
            origin,
            client,
        })
    }

    pub(super) fn draft(&self, title: &str, description: &str) -> Value {
        let p = &self.profile;
        let deadline = Utc::now() + Duration::minutes(30);
        let remote = json!({
            "request_id":Uuid::new_v4(),"mandate_id":p.remote_mandate_id,"specialist_id":p.remote_specialist,
            "objective":"Track an operator-requested Linear issue","capability":"linear_track",
            "resource":{"kind":"linear_project","id":p.linear_project_id},
            "acceptance_criteria":["linear_issue_readback_verified"],"deadline":deadline,
            "budget":{"micro_usd":0,"calls":8,"turns":4,"delegation_depth":0},
            "payload":{"kind":"linear_track","title":title,"description":description,"lookup_title":title}
        });
        json!({
            "request_id":Uuid::new_v4(),"mandate_id":p.local_mandate_id,"specialist_id":p.local_specialist,
            "objective":"Ask the Linear specialist to track this issue","capability":"delegate",
            "resource":{"kind":"peer","id":p.remote_peer},"acceptance_criteria":["delegated_result_reported"],
            "deadline":deadline,"budget":{"micro_usd":0,"calls":12,"turns":8,"delegation_depth":1},
            "payload":{"kind":"delegate","peer":p.remote_peer,"request":remote}
        })
    }

    async fn call(&self, path: &str, body: Option<&Value>) -> Result<Value> {
        let url = self.origin.join(path)?;
        let call = match body {
            Some(body) => self.client.post(url).json(body),
            None => self.client.get(url),
        };
        let response = call
            .bearer_auth(&self.token)
            .header("A2A-Version", "1.0")
            .send()
            .await?;
        ensure!(
            response.status().is_success(),
            "companion returned HTTP {}",
            response.status()
        );
        Ok(response.json().await?)
    }

    pub(super) async fn submit(&self, body: &Value) -> Result<Uuid> {
        let task = self.call("/v1/tasks", Some(body)).await?;
        serde_json::from_value(task["contract"]["id"].clone()).context("missing local task ID")
    }

    pub(super) async fn task(&self, id: Uuid) -> Result<Value> {
        self.call(&format!("/v1/tasks/{id}"), None).await
    }

    pub(super) async fn answer(&self, id: Uuid, revision: i64, answer: &str) -> Result<()> {
        let body = json!({"message":{"messageId":format!("{id}:approval:{revision}"),"contextId":id,
            "taskId":id,"role":"ROLE_USER","parts":[{"data":{"answer":answer}}]},
            "configuration":{"returnImmediately":true}});
        self.call("/message:send", Some(&body)).await?;
        Ok(())
    }

    pub(super) async fn issue_url(&self, id: Uuid) -> Result<String> {
        let task = self.call(&format!("/tasks/{id}"), None).await?;
        verified_url(&task).context("completed without a verified Linear issue URL")
    }
}

fn artifacts(task: &Value) -> impl Iterator<Item = &Value> {
    task["artifacts"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|a| a["parts"].as_array().into_iter().flatten())
        .filter_map(|p| p.get("data"))
}

fn verified_url(task: &Value) -> Option<String> {
    if task["status"]["state"] != "TASK_STATE_COMPLETED" {
        return None;
    }
    for delegate in artifacts(task) {
        let result = &delegate["reported_result"];
        if result["status"]["state"] != "TASK_STATE_COMPLETED" {
            continue;
        }
        for evidence in artifacts(result) {
            if evidence["readback_verified"] == true
                && evidence["issue_id"]
                    .as_str()
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .is_some()
            {
                let url = Url::parse(evidence["url"].as_str()?).ok()?;
                if url.scheme() == "https" && url.username().is_empty() && url.password().is_none()
                {
                    return Some(url.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_requires_completed_peer_readback_evidence() {
        let url = "https://linear.app/example/issue/OPS-3";
        let mut local = json!({"status":{"state":"TASK_STATE_COMPLETED"},"artifacts":[{"parts":[{"data":{
            "reported_result":{"status":{"state":"TASK_STATE_COMPLETED"},"artifacts":[{"parts":[{"data":{
                "readback_verified":true,"issue_id":Uuid::new_v4(),"url":url
            }}]}]}
        }}]}]});
        assert_eq!(verified_url(&local).as_deref(), Some(url));
        let result = &mut local["artifacts"][0]["parts"][0]["data"]["reported_result"];
        result["artifacts"][0]["parts"][0]["data"]["readback_verified"] = json!(false);
        assert_eq!(verified_url(&local), None);
    }
}
