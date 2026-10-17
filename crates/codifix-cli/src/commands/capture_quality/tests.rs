// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

fn case() -> cq::Case {
    serde_json::from_value(json!({"id":"a","source":"synthetic","category":"clear_type","task_title":"Architecture decision","user_prompt":"Record architecture","agent_summary":"Modules form the system","candidate_text":"Architecture uses isolated modules.","expected_type":"architecture","expected_durable":true,"rationale":"Test"})).unwrap()
}
fn response() -> Value {
    let probabilities = codifix_record::MemoryType::ALL
        .iter()
        .map(|kind| {
            (
                kind.to_string(),
                json!(if kind == &codifix_record::MemoryType::Architecture {
                    1.0
                } else {
                    0.0
                }),
            )
        })
        .chain(std::iter::once(("unsure".into(), json!(0.0))))
        .collect::<serde_json::Map<_, _>>();
    json!({"model":cq::MODEL,"answers":{"memory_type":{"type":"choice","choice":"architecture","probabilities":probabilities,"confidence":0.9},"lasting_value":{"type":"noul","noul":0.8}},"usage":{"input_tokens":100,"output_tokens":20},"metadata":"SECRET-do-not-retain"})
}
fn server(status: &str, body: &str, delay: Duration) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let status = status.to_owned();
    let body = body.to_owned();
    let join = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let n = stream.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        thread::sleep(delay);
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\nLocation: http://127.0.0.1:1/secret\r\n\r\n{body}",
            body.len()
        );
        String::from_utf8_lossy(&bytes).into_owned()
    });
    (url, join)
}
#[test]
fn genuine_extractor_parity_and_explicit_override() {
    let mut c = case();
    let request:CaptureTaskRequest=serde_json::from_value(json!({"project":"capture-quality-pilot","writer_id":"capture-quality-pilot","task_title":c.task_title,"user_prompt":c.user_prompt,"agent_summary":c.agent_summary,"notes":[c.candidate_text]})).unwrap();
    assert_eq!(
        rules(&c).unwrap().label,
        codifix_curate::ingest::extract_candidates(&request)[0]
            .memory_type
            .to_string()
    );
    c.explicit_type = Some(codifix_record::MemoryType::Decision);
    assert_eq!(rules(&c).unwrap().label, "decision");
    let mut explicit_request = request;
    explicit_request.notes.clear();
    explicit_request.structured_candidates=vec![serde_json::from_value(json!({"canonical_text":c.candidate_text,"summary":c.agent_summary,"memory_type":"decision"})).unwrap()];
    assert_eq!(
        rules(&c).unwrap().label,
        codifix_curate::ingest::extract_candidates(&explicit_request)[0]
            .memory_type
            .to_string()
    );
    assert_eq!(cq::request(&c)["state"]["explicit_type"], "decision");
    let suite = Suite {
        schema_version: 1,
        name: "test".into(),
        label_status: "agent_reviewed".into(),
        cases: vec![c.clone()],
    };
    let mut o = observation(&c, "rules", "rules", None).unwrap();
    o.prediction = Some(rules(&c).unwrap());
    let score = cq::score(&suite, &[o], "rules");
    assert_eq!(score.inferred.total, 0);
    assert_eq!(score.explicit.correct, 1);
}
#[tokio::test]
async fn fake_http_success_and_bound_replay() {
    let c = case();
    let request = cq::request(&c);
    let client = provider_client(Duration::from_secs(2)).unwrap();
    let (url, join) = server("200 OK", &response().to_string(), Duration::ZERO);
    let result = live(
        &client,
        &url,
        "SECRET",
        &request,
        observation(&c, "jev", "live", Some(&request)).unwrap(),
    )
    .await;
    let sent = join.join().unwrap();
    assert!(sent.contains("Bearer SECRET"));
    assert!(!sent.contains("expected_type"));
    assert!(result.error.is_none());
    assert_eq!(result.prediction.as_ref().unwrap().label, "architecture");
    assert_eq!(result.usage.as_ref().unwrap()["input_tokens"], 100);
    assert!(result.known_input_cost_usd.unwrap() > 0.0);
    assert!(!serde_json::to_string(&result).unwrap().contains("SECRET"));
    let suite = Suite {
        schema_version: 1,
        name: "test".into(),
        label_status: "agent_reviewed".into(),
        cases: vec![c.clone()],
    };
    let mut run = Run {
        schema_version: 1,
        suite,
        suite_hash: "hash".into(),
        prompt_version: cq::PROMPT_VERSION.into(),
        dry_run: false,
        completed: true,
        backend: "jev".into(),
        requests: BTreeMap::new(),
        observations: vec![result],
        metrics: BTreeMap::new(),
    };
    let replay = replay_case(
        &run,
        observation(&c, "jev", "replay", Some(&request)).unwrap(),
    )
    .unwrap();
    assert_eq!(replay.source, "replay");
    assert!(replay.latency_ms.is_none() && replay.known_input_cost_usd.is_none());
    let mut changed = c.clone();
    changed.candidate_text.push('!');
    assert!(
        replay_case(
            &run,
            observation(&changed, "jev", "replay", Some(&cq::request(&changed))).unwrap()
        )
        .is_err()
    );
    run.observations[0].raw_response.as_mut().unwrap()["answers"]["lasting_value"]["noul"] =
        json!(0.2);
    assert!(
        replay_case(
            &run,
            observation(&c, "jev", "replay", Some(&request)).unwrap()
        )
        .is_err()
    );
}
#[tokio::test]
async fn safe_http_error_malformed_redirect_timeout_and_model_mismatch() {
    let c = case();
    let request = cq::request(&c);
    for (status, body, delay, timeout, expected) in [
        (
            "401 Unauthorized",
            "SECRET-provider-error".into(),
            Duration::ZERO,
            Duration::from_secs(2),
            "provider HTTP status 401",
        ),
        (
            "302 Found",
            "SECRET-redirect".into(),
            Duration::ZERO,
            Duration::from_secs(2),
            "provider HTTP status 302",
        ),
        (
            "200 OK",
            "SECRET-not-json".into(),
            Duration::ZERO,
            Duration::from_secs(2),
            "provider returned invalid JSON",
        ),
        (
            "200 OK",
            json!({"model":"wrong"}).to_string(),
            Duration::ZERO,
            Duration::from_secs(2),
            "provider model mismatch",
        ),
        (
            "200 OK",
            response().to_string(),
            Duration::from_millis(150),
            Duration::from_millis(30),
            "provider request timed out",
        ),
    ] {
        let (url, join) = server(status, &body, delay);
        let result = live(
            &provider_client(timeout).unwrap(),
            &url,
            "SECRET",
            &request,
            observation(&c, "jev", "live", Some(&request)).unwrap(),
        )
        .await;
        join.join().unwrap();
        assert_eq!(result.error.as_deref(), Some(expected));
        assert!(result.raw_response.is_none());
        assert!(!serde_json::to_string(&result).unwrap().contains("SECRET"));
    }
}
#[tokio::test]
async fn dry_run_does_not_build_transport_and_existing_evidence_is_refused() {
    let root = std::env::temp_dir().join(format!("capture-quality-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let suite = root.join("suite.json");
    fs::write(
        &suite,
        json!({"schema_version":1,"name":"test","label_status":"agent_reviewed","cases":[case()]})
            .to_string(),
    )
    .unwrap();
    let out = root.join("run");
    let args = || EvalCaptureQualityArgs {
        suite: suite.clone(),
        out: out.clone(),
        backend: "both".into(),
        dry_run: true,
        replay: None,
    };
    handle(args()).await.unwrap();
    let evidence = fs::read(out.join("run.json")).unwrap();
    let run: Run = serde_json::from_slice(&evidence).unwrap();
    assert_eq!(run.observations.len(), 2);
    assert!(run.observations[1].prediction.is_none());
    assert_eq!(run.observations[1].source, "dry_run");
    assert_eq!(run.requests.len(), 1);
    assert!(!run.metrics.contains_key("jev"));
    assert!(handle(args()).await.is_err());
    assert_eq!(fs::read(out.join("run.json")).unwrap(), evidence);
    fs::remove_dir_all(root).unwrap();
}
