// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adapter for the existing external-retriever JSON protocol. No filesystem,
//! provider access, question-specific summaries, or Memory ranking fallback.
use super::baselines::{self, Document};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Handover,
    Lexical,
    Vector,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub project: String,
    pub condition: String,
    pub mode: Mode,
    pub documents: Vec<Document>,
    #[serde(default)]
    pub embeddings: Vec<Vec<f64>>,
    pub top_k: usize,
    pub bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema_version: u8,
    pub project: String,
    pub condition: String,
    pub query: String,
    pub limit: i64,
    pub item_id: String,
    pub context: Context,
    /// The campaign mediator may supply a query embedding. The legacy runner
    /// does not; vector mode fails closed rather than inventing one.
    #[serde(default)]
    pub query_embedding: Option<Vec<f64>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub fixture_path: String,
    pub hidden_facts: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub schema_version: u8,
    pub results: Vec<Hit>,
}

#[derive(Debug, Serialize)]
pub struct Hit {
    pub id: String,
    pub score: f64,
    pub text: String,
    pub citations: Vec<String>,
}

pub fn retrieve(config: &Configuration, request: &Request) -> Result<Response> {
    ensure!(
        request.schema_version == 1,
        "unsupported retriever protocol"
    );
    ensure!(
        !config.project.is_empty()
            && request.project == config.project
            && !config.condition.is_empty()
            && request.condition == config.condition,
        "retriever scope mismatch"
    );
    ensure!(
        request.context.hidden_facts.is_empty(),
        "hidden facts are not retrieval context"
    );
    // fixture_path is intentionally never followed, even if an old runner sends
    // a host path. Only the explicitly prepared corpus enters this comparator.
    ensure!(
        !request.item_id.is_empty() && request.query.len() <= 65536,
        "invalid retrieval request"
    );
    ensure!(
        (1..=100).contains(&request.limit)
            && (1..=100).contains(&config.top_k)
            && (1..=1_000_000).contains(&config.bytes),
        "invalid retrieval budget"
    );
    let mut ids = BTreeSet::new();
    ensure!(
        config
            .documents
            .iter()
            .all(|d| !d.id.is_empty() && ids.insert(&d.id)),
        "invalid corpus identities"
    );
    let top_k = config.top_k.min(request.limit as usize);
    let hits = match config.mode {
        Mode::Handover => {
            ensure!(
                config.documents.len() <= top_k,
                "handover exceeds common result limit"
            );
            baselines::handover(&config.documents, config.bytes)?
        }
        Mode::Lexical => {
            baselines::lexical(&config.documents, &request.query, top_k, config.bytes)?
        }
        Mode::Vector => baselines::vector(
            &config.documents,
            &config.embeddings,
            request
                .query_embedding
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("query embedding required"))?,
            top_k,
            config.bytes,
        )?,
    };
    Ok(Response {
        schema_version: 1,
        results: hits
            .into_iter()
            .map(|h| Hit {
                citations: vec![h.id.clone()],
                id: h.id,
                score: h.score,
                text: h.text,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> (Configuration, Request) {
        let config = serde_json::from_value(serde_json::json!({
            "project":"synthetic", "condition":"external-retriever", "mode":"lexical",
            "documents":[{"id":"runbook","text":"retry after reconciliation"}],
            "top_k":8,"bytes":1024
        }))
        .expect("configuration");
        let request = serde_json::from_value(serde_json::json!({
            "schema_version":1,"project":"synthetic","condition":"external-retriever",
            "query":"retry","limit":8,"item_id":"question-1",
            "context":{"fixture_path":"/not-opened/private","hidden_facts":[]}
        }))
        .expect("request");
        (config, request)
    }

    #[test]
    fn external_protocol_preserves_evidence_and_rejects_scope_and_label_leaks() {
        let (config, mut request) = fixtures();
        let response = retrieve(&config, &request).expect("lexical result");
        assert_eq!(response.results[0].citations, ["runbook"]);
        request.project = "another-condition".into();
        assert!(retrieve(&config, &request).is_err());
        request.project = config.project.clone();
        request.context.hidden_facts.push("answer key".into());
        assert!(retrieve(&config, &request).is_err());
    }

    #[test]
    fn vector_requires_real_input_and_handover_does_not_silently_drop_facts() {
        let (mut config, mut request) = fixtures();
        config.mode = Mode::Vector;
        config.embeddings = vec![vec![1.0, 0.0]];
        assert!(retrieve(&config, &request).is_err());
        request.query_embedding = Some(vec![1.0, 0.0]);
        assert_eq!(
            retrieve(&config, &request)
                .expect("vector result")
                .results
                .len(),
            1
        );
        config.mode = Mode::Handover;
        config.bytes = 1;
        assert!(retrieve(&config, &request).is_err());
    }
}
