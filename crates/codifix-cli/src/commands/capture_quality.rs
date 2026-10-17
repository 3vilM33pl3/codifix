// SPDX-License-Identifier: AGPL-3.0-or-later
//! Opt-in local pilot. Deliberately independent of ApiClient and service configuration.
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use codifix_eval::capture_quality::{self as cq, Observation, Prediction, Run, Suite};
use codifix_record::CaptureTaskRequest;
use reqwest::{Client, redirect::Policy};
use serde_json::{Value, json};

use super::runtime::EvalCaptureQualityArgs;

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const TIMEOUT: Duration = Duration::from_secs(20);
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

fn rules(case: &cq::Case) -> Result<Prediction> {
    // The exact production extractor and input fields, never a copied keyword classifier.
    let mut value = json!({
        "project":"capture-quality-pilot", "writer_id":"capture-quality-pilot",
        "task_title":case.task_title,"user_prompt":case.user_prompt,
        "agent_summary":case.agent_summary,"notes":[case.candidate_text]
    });
    if let Some(kind) = &case.explicit_type {
        value["notes"] = json!([]);
        value["structured_candidates"] = json!([{"canonical_text":case.candidate_text,"summary":case.agent_summary,"memory_type":kind}]);
    }
    let request: CaptureTaskRequest = serde_json::from_value(value)?;
    let candidates = codifix_curate::ingest::extract_candidates(&request);
    let candidate = candidates
        .first()
        .context("extractor returned no candidate")?;
    Ok(Prediction {
        label: candidate.memory_type.to_string(),
        probabilities: BTreeMap::new(),
        decision_confidence: None,
        durable_probability: None,
    })
}
fn observation(
    case: &cq::Case,
    backend: &str,
    source: &str,
    request: Option<&Value>,
) -> Result<Observation> {
    Ok(Observation {
        case_id: case.id.clone(),
        backend: backend.into(),
        source: source.into(),
        input_hash: cq::hash(&serde_json::to_vec(&cq::input(case))?),
        request_hash: request
            .map(|r| serde_json::to_vec(r).map(|bytes| cq::hash(&bytes)))
            .transpose()?,
        prompt_hash: request.map(cq::prompt_hash).transpose()?,
        model: (backend == "jev").then(|| cq::MODEL.into()),
        prediction: None,
        error: None,
        latency_ms: None,
        usage: None,
        known_input_cost_usd: None,
        raw_response: None,
        raw_response_hash: None,
    })
}
fn provider_client(timeout: Duration) -> Result<Client> {
    Client::builder()
        .timeout(timeout)
        .redirect(Policy::none())
        .build()
        .context("build capture-quality HTTP client")
}
// Endpoint injection is private and used only by local fake HTTP unit tests.
async fn send(client: &Client, endpoint: &str, key: &str, request: &Value) -> Result<Value> {
    let response = client
        .post(endpoint)
        .bearer_auth(key)
        .json(request)
        .send()
        .await
        .map_err(|error| {
            anyhow::anyhow!(if error.is_timeout() {
                "provider request timed out"
            } else {
                "provider transport failure"
            })
        })?;
    ensure!(
        response.status().is_success(),
        "provider HTTP status {}",
        response.status().as_u16()
    );
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= MAX_RESPONSE_BYTES as u64),
        "provider response too large"
    );
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("provider response read failure"))?
    {
        ensure!(
            bytes.len() + chunk.len() <= MAX_RESPONSE_BYTES,
            "provider response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("provider returned invalid JSON"))
}
fn retained_usage(response: &Value) -> Option<Value> {
    // Allowlist numeric accounting only; arbitrary provider metadata is not report content.
    let usage = response.get("usage")?;
    let input = usage.get("input_tokens").and_then(Value::as_u64);
    let output = usage.get("output_tokens").and_then(Value::as_u64);
    (input.is_some() || output.is_some())
        .then(|| json!({"input_tokens":input,"output_tokens":output}))
}
fn safe_response(response: &Value) -> Value {
    // Store raw successful protocol answers, model and numeric usage, never headers or
    // arbitrary metadata that might echo credentials. Exact retained bytes are hashed.
    json!({"model":response["model"],"answers":{
        "memory_type":{"type":response["answers"]["memory_type"]["type"],"choice":response["answers"]["memory_type"]["choice"],"probabilities":response["answers"]["memory_type"]["probabilities"],"confidence":response["answers"]["memory_type"]["confidence"]},
        "lasting_value":{"type":response["answers"]["lasting_value"]["type"],"noul":response["answers"]["lasting_value"]["noul"]}},"usage":retained_usage(response)})
}
async fn live(
    client: &Client,
    endpoint: &str,
    key: &str,
    request: &Value,
    mut result: Observation,
) -> Observation {
    let started = Instant::now();
    match send(client, endpoint, key, request)
        .await
        .and_then(|value| cq::parse_response(&value).map(|prediction| (value, prediction)))
    {
        Ok((response, prediction)) => {
            result.prediction = Some(prediction);
            result.usage = retained_usage(&response);
            result.known_input_cost_usd = result
                .usage
                .as_ref()
                .and_then(|u| u["input_tokens"].as_u64())
                .map(|tokens| tokens as f64 * cq::INPUT_USD_PER_MILLION / 1_000_000.0);
            let raw = safe_response(&response);
            // Value serialization is infallible; hash the same bytes replay verifies.
            result.raw_response_hash = Some(cq::hash(raw.to_string().as_bytes()));
            result.raw_response = Some(raw);
        }
        Err(error) => {
            result.error = Some(error.to_string());
        }
    }
    result.latency_ms = Some(started.elapsed().as_millis().min(u64::MAX as u128) as u64);
    result
}
fn replay_case(run: &Run, mut result: Observation) -> Result<Observation> {
    let saved = run
        .observations
        .iter()
        .filter(|o| o.backend == "jev" && o.case_id == result.case_id)
        .collect::<Vec<_>>();
    ensure!(
        saved.len() == 1,
        "replay needs exactly one Jev observation per case"
    );
    let saved = saved[0];
    ensure!(
        saved.input_hash == result.input_hash
            && saved.request_hash == result.request_hash
            && saved.prompt_hash == result.prompt_hash
            && saved.model == result.model,
        "replay input/request/prompt/model binding mismatch"
    );
    if let Some(raw) = &saved.raw_response {
        ensure!(
            saved.error.is_none(),
            "replay response conflicts with saved error"
        );
        ensure!(
            saved.raw_response_hash.as_deref()
                == Some(cq::hash(raw.to_string().as_bytes()).as_str()),
            "replay response hash mismatch"
        );
        result.prediction = Some(cq::parse_response(raw)?);
        result.usage = retained_usage(raw);
        result.raw_response = Some(raw.clone());
        result.raw_response_hash = saved.raw_response_hash.clone();
    } else {
        ensure!(
            saved.prediction.is_none() && saved.error.is_some(),
            "replay lacks raw successful response or error"
        );
        // Do not propagate arbitrary user-edited error strings into reports.
        result.error = Some("replayed provider case failure (see original run)".into());
    }
    Ok(result)
}
fn load_replay(path: &Path, suite_hash: &str) -> Result<Run> {
    let path = if path.is_dir() {
        path.join("run.json")
    } else {
        path.to_path_buf()
    };
    let run: Run = serde_json::from_slice(&fs::read(path).context("read replay run")?)
        .context("parse replay run")?;
    ensure!(
        run.schema_version == 1 && !run.dry_run && run.prompt_version == cq::PROMPT_VERSION,
        "incompatible replay run"
    );
    ensure!(
        run.suite_hash == suite_hash,
        "replay exact fixture hash mismatch"
    );
    Ok(run)
}
fn persist(run: &Run, directory: &Path) -> Result<()> {
    for (name, bytes) in [
        ("run.json", serde_json::to_vec_pretty(run)?),
        ("report.md", cq::markdown(run)?.into_bytes()),
    ] {
        let temporary = directory.join(format!(".{name}.tmp"));
        fs::write(&temporary, bytes).context("write capture-quality evidence checkpoint")?;
        fs::rename(temporary, directory.join(name))
            .context("replace capture-quality evidence checkpoint")?;
    }
    Ok(())
}
pub(super) async fn handle(args: EvalCaptureQualityArgs) -> Result<()> {
    let bytes = fs::read(&args.suite).context("read capture-quality suite")?;
    let suite: Suite = serde_json::from_slice(&bytes).context("parse capture-quality suite")?;
    suite.validate()?;
    let suite_hash = cq::hash(&bytes);
    let use_jev = args.backend == "jev" || args.backend == "both";
    ensure!(
        args.replay.is_none() || use_jev,
        "--replay requires --backend jev or both"
    );
    let replay = args
        .replay
        .as_deref()
        .map(|path| load_replay(path, &suite_hash))
        .transpose()?;
    // Credentials are a preflight: no output directory or request before this succeeds.
    let key = if use_jev && !args.dry_run && replay.is_none() {
        let key = std::env::var("TYPESAFE_API_KEY")
            .ok()
            .filter(|key| !key.trim().is_empty())
            .context("live Jev capture-quality requires TYPESAFE_API_KEY")?;
        ensure!(
            reqwest::header::HeaderValue::from_str(&format!("Bearer {key}")).is_ok(),
            "invalid TYPESAFE_API_KEY header"
        );
        Some(key)
    } else {
        None
    };
    let client = if key.is_some() {
        Some(provider_client(TIMEOUT)?)
    } else {
        None
    };
    let mut observations = Vec::new();
    let mut requests = BTreeMap::new();
    for case in &suite.cases {
        if args.backend == "rules" || args.backend == "both" {
            let mut result = observation(case, "rules", "rules", None)?;
            result.prediction = Some(rules(case)?);
            observations.push(result);
        }
        if use_jev {
            let request = cq::request(case);
            requests.insert(case.id.clone(), request.clone());
            let source = if args.dry_run {
                "dry_run"
            } else if replay.is_some() {
                "replay"
            } else {
                "live"
            };
            let result = observation(case, "jev", source, Some(&request))?;
            observations.push(if let Some(saved) = &replay {
                replay_case(saved, result)?
            } else {
                result
            });
        }
    }
    // All replay bindings are checked before creating evidence. Atomic directory creation
    // refuses overwrite, including concurrent invocations targeting the same directory.
    if let Some(parent) = args.out.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).context("create output parent")?;
    }
    fs::create_dir(&args.out)
        .context("create fresh capture-quality output directory (must not exist)")?;
    let mut metrics = BTreeMap::new();
    if args.backend != "jev" {
        metrics.insert("rules".into(), cq::score(&suite, &observations, "rules"));
    }
    let mut run = Run {
        schema_version: 1,
        suite,
        suite_hash,
        prompt_version: cq::PROMPT_VERSION.into(),
        dry_run: args.dry_run,
        completed: false,
        backend: args.backend,
        requests,
        observations,
        metrics,
    };
    // Save exact inputs before any paid request, and save each result before the next.
    // Interrupted runs retain completed responses, usage and errors without implicit retries.
    persist(&run, &args.out)?;
    if let (Some(client), Some(key)) = (&client, &key) {
        for index in 0..run.observations.len() {
            let result = &run.observations[index];
            if result.backend == "jev" {
                let request = run
                    .requests
                    .get(&result.case_id)
                    .context("missing prepared request")?;
                run.observations[index] =
                    live(client, ENDPOINT, key, request, result.clone()).await;
                persist(&run, &args.out)?;
            }
        }
    }
    if use_jev && !args.dry_run {
        run.metrics.insert(
            "jev".into(),
            cq::score(&run.suite, &run.observations, "jev"),
        );
    }
    run.completed = true;
    persist(&run, &args.out)?;
    println!("Capture-quality evidence: {}", args.out.display());
    Ok(())
}

#[cfg(test)]
mod tests;
