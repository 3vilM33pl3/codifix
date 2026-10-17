// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use serde_json::json;
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Command, Output},
};

struct Fixture {
    root: PathBuf,
    suite: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("capture-quality-cli-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let suite = root.join("suite.json");
        fs::write(&suite,json!({"schema_version":1,"name":"binary-test","label_status":"agent_reviewed","cases":[{"id":"a","source":"synthetic","category":"clear_type","task_title":"Architecture","user_prompt":"Record system structure","agent_summary":"Components are isolated.","candidate_text":"Architecture isolates components.","expected_type":"architecture","expected_durable":true,"rationale":"Test"}]}).to_string()).unwrap();
        Self { root, suite }
    }
    fn invoke(&self, backend: &str, dry_run: bool, out: &str, proxy: &str, config: &str) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codifix"));
        command
            .env_remove("TYPESAFE_API_KEY")
            .env("HTTPS_PROXY", proxy)
            .env("HTTP_PROXY", proxy)
            .env("ALL_PROXY", proxy)
            .env("NO_PROXY", "")
            .arg("--config")
            .arg(config)
            .args(["eval", "capture-quality", "--suite"])
            .arg(&self.suite)
            .args(["--backend", backend, "--out"])
            .arg(self.root.join(out));
        if dry_run {
            command.arg("--dry-run");
        }
        command.output().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn actual_cli_rules_and_jev_dry_run_ignore_broken_config_key_and_network() {
    let fixture = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let invalid = fixture.root.join("invalid.toml");
    fs::write(&invalid, "not = [ valid config").unwrap();
    for (backend, dry_run, out, config) in [
        (
            "rules",
            false,
            "rules",
            "/nonexistent/capture-quality-config.toml",
        ),
        ("both", true, "dry", invalid.to_str().unwrap()),
    ] {
        let result = fixture.invoke(backend, dry_run, out, &proxy, config);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let run: codifix_eval::capture_quality::Run =
            serde_json::from_slice(&fs::read(fixture.root.join(out).join("run.json")).unwrap())
                .unwrap();
        assert!(run.completed);
        assert!(run.metrics.contains_key("rules"));
        assert!(!run.metrics.contains_key("jev"));
        if dry_run {
            assert!(
                run.observations
                    .iter()
                    .filter(|o| o.backend == "jev")
                    .all(|o| o.source == "dry_run" && o.error.is_none() && o.latency_ms.is_none())
            );
        }
    }
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
#[test]
fn actual_cli_missing_key_fails_before_output_or_network() {
    let fixture = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let output = fixture.invoke(
        "jev",
        false,
        "missing-key",
        &proxy,
        "/nonexistent/capture-quality-config.toml",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires TYPESAFE_API_KEY"));
    assert!(!fixture.root.join("missing-key").exists());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn actual_cli_replay_is_offline_and_exact_fixture_bound() {
    use codifix_eval::capture_quality as cq;
    let fixture = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    assert!(
        fixture
            .invoke("jev", true, "preview", &proxy, "/nonexistent/config")
            .status
            .success()
    );
    let path = fixture.root.join("preview/run.json");
    let mut original: cq::Run = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let probabilities = codifix_record::MemoryType::ALL
        .iter()
        .map(|kind| {
            (
                kind.to_string(),
                json!(if *kind == codifix_record::MemoryType::Architecture {
                    1.0
                } else {
                    0.0
                }),
            )
        })
        .chain(std::iter::once(("unsure".into(), json!(0.0))))
        .collect::<serde_json::Map<_, _>>();
    let raw = json!({"model":cq::MODEL,"answers":{"memory_type":{"type":"choice","choice":"architecture","probabilities":probabilities,"confidence":0.8},"lasting_value":{"type":"noul","noul":0.9}},"usage":{"input_tokens":100,"output_tokens":10}});
    original.dry_run = false;
    let observation = &mut original.observations[0];
    observation.source = "live".into();
    observation.prediction = Some(cq::parse_response(&raw).unwrap());
    observation.raw_response_hash = Some(cq::hash(raw.to_string().as_bytes()));
    observation.raw_response = Some(raw);
    observation.latency_ms = Some(12);
    observation.known_input_cost_usd = Some(0.0000042);
    fs::write(&path, serde_json::to_vec_pretty(&original).unwrap()).unwrap();
    let replay = |out: &str| {
        Command::new(env!("CARGO_BIN_EXE_codifix"))
            .env_remove("TYPESAFE_API_KEY")
            .env("HTTPS_PROXY", &proxy)
            .env("HTTP_PROXY", &proxy)
            .env("ALL_PROXY", &proxy)
            .env("NO_PROXY", "")
            .args([
                "--config",
                "/nonexistent/config",
                "eval",
                "capture-quality",
                "--suite",
            ])
            .arg(&fixture.suite)
            .args(["--backend", "jev", "--replay"])
            .arg(&path)
            .arg("--out")
            .arg(fixture.root.join(out))
            .output()
            .unwrap()
    };
    let output = replay("replay");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run: cq::Run =
        serde_json::from_slice(&fs::read(fixture.root.join("replay/run.json")).unwrap()).unwrap();
    assert_eq!(run.metrics["jev"].inferred.correct, 1);
    assert_eq!(run.observations[0].source, "replay");
    assert!(
        run.observations[0].latency_ms.is_none()
            && run.observations[0].known_input_cost_usd.is_none()
    );
    let mut bytes = fs::read(&fixture.suite).unwrap();
    bytes.push(b'\n');
    fs::write(&fixture.suite, bytes).unwrap();
    let rejected = replay("changed");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("exact fixture hash mismatch"));
    assert!(!fixture.root.join("changed").exists());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
