// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit synthetic diagnostic only; no production endpoint override or prompt
//! API is exposed. Run inside the independently enforced network-none sandbox.
#![allow(clippy::unwrap_used)]

use crate::{Failure, LinuxArchitecture, ProbeConfig, artifact, process, protocol};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    time::timeout,
};

const TOKEN: &str = "synthetic-local-probe-not-a-valid-provider-key";
const QUESTION: &str = "Synthetic routing check. Return the supplied synthetic response unchanged.";
const ANSWER: &str = "Synthetic provider result; no real model was called.";
const SYSTEM: &str = "This is a synthetic local transport test. No tools or external actions.";

// Private test identity only. Never replace the official production pins or
// accept a caller/environment-provided digest to make an arbitrary binary pass.
const PATCHED_AMD64_SHA256: &str =
    "912366d4178bd7c12ccf4401b6241b1dcb6f8cdbd66302bc1764a1cec9dfe88c";

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArtifactProfile {
    Official,
    NoTools1,
}

impl ArtifactProfile {
    fn label(self) -> &'static str {
        match self {
            Self::Official => "upstream-0.5.1",
            Self::NoTools1 => "0.5.1+memory.no-tools.1",
        }
    }
}

fn check_tools(
    body: &Value,
    explicit_tool_off: bool,
    profile: ArtifactProfile,
) -> Result<(), &'static str> {
    if profile == ArtifactProfile::NoTools1 {
        return match body.get("tools") {
            None => Ok(()),
            Some(Value::Array(tools)) if tools.is_empty() => Ok(()),
            _ => Err("unexpected patched tool declaration"),
        };
    }
    // Preserve the original regression: off tiers change discovery, not the
    // unconditional registrations. This is not a passed tool-free gate.
    let tools = body["tools"]
        .as_array()
        .ok_or("upstream tool regression changed")?;
    let expected = if explicit_tool_off {
        ["manage_skill", "submit_plan"]
    } else {
        ["xdev", "submit_plan"]
    };
    if tools.len() != 2 || tools[0]["name"] != expected[0] || tools[1]["name"] != expected[1] {
        return Err("unexpected upstream tool declaration");
    }
    Ok(())
}

/// The marker is not an attestation. Docker's independently supplied boundary is
/// mandatory. These extra checks catch accidental invocation on the host.
fn require_sandbox() {
    assert_eq!(
        std::env::var("MEMORY_PI_SANDBOXED_PROBE").unwrap(),
        "network-none-read-only-root"
    );
    assert_ne!(rustix::process::geteuid().as_raw(), 0);
    let interfaces: Vec<_> = std::fs::read_dir("/sys/class/net")
        .unwrap()
        .map(|p| p.unwrap().file_name())
        .collect();
    assert_eq!(interfaces, ["lo"]);
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    assert!(status.lines().any(|line| line == "NoNewPrivs:\t1"));
    assert!(
        status
            .lines()
            .any(|line| line == "CapEff:\t0000000000000000")
    );
}

async fn serve_one(
    stream: TcpStream,
    status: u16,
    explicit_tool_off: bool,
    profile: ArtifactProfile,
) -> Result<(), &'static str> {
    let mut reader = BufReader::new(stream);
    let mut header = Vec::new();
    // Read only bounded HTTP framing. No request/header/provider body appears in
    // assertions or stdout; an error is a fixed stage name.
    loop {
        let byte = reader.read_u8().await.map_err(|_| "http header read")?;
        header.push(byte);
        if header.len() > 8192 {
            return Err("http header bound");
        }
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let header = std::str::from_utf8(&header).map_err(|_| "http header encoding")?;
    let mut lines = header.split("\r\n");
    if lines.next() != Some("POST /v1/responses HTTP/1.1") {
        return Err("http path");
    }
    let mut length = None;
    let mut authenticated = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err("duplicate http length");
            }
            length = Some(value.trim().parse::<usize>().map_err(|_| "http length")?);
        }
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err("unexpected transfer encoding");
        }
        if name.eq_ignore_ascii_case("authorization") {
            if authenticated || value.trim() != format!("Bearer {TOKEN}") {
                return Err("http fixture auth");
            }
            authenticated = true;
        }
    }
    let length = length
        .filter(|n| (1..=65_536).contains(n))
        .ok_or("http body bound")?;
    if !authenticated {
        return Err("http fixture auth missing");
    }
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|_| "http body read")?;
    let body: Value = serde_json::from_slice(&body).map_err(|_| "http json")?;
    if body["model"] != mem_supervisor::DEFAULT_MODEL {
        return Err("wrong model");
    }
    if body["stream"] != true || body["store"] != false {
        return Err("stream or storage mode");
    }
    check_tools(&body, explicit_tool_off, profile)?;
    let tools = body["tools"].as_array().map_or(&[][..], Vec::as_slice);
    println!(
        "{}",
        json!({"synthetic_request_tool_names": tools.iter().take(16).map(|tool| tool["name"].as_str().filter(|name| name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')).unwrap_or("unrecognized")).collect::<Vec<_>>()})
    );
    // Pinned app::build_system_prompt appends a current-date line even to an
    // explicit system prompt. No path or other appended context is accepted.
    let instructions = body["instructions"]
        .as_str()
        .ok_or("missing system context")?;
    let date = instructions
        .strip_prefix(&format!("{SYSTEM}\nCurrent date and time: "))
        .ok_or("unreviewed system context")?;
    if date.is_empty()
        || date.len() > 128
        || !date.is_ascii()
        || date.chars().any(char::is_control)
        || date.contains('/')
    {
        return Err("unexpected date context");
    }
    if body["max_output_tokens"] != 800 {
        return Err("unbounded output tokens");
    }
    let input = body["input"].as_array().ok_or("missing input")?;
    if input.len() != 1 || input[0]["role"] != "user" {
        return Err("unexpected conversation context");
    }
    if input[0]["content"] != QUESTION
        && input[0]["content"] != json!([{"type":"input_text","text":QUESTION}])
    {
        return Err("unexpected question");
    }
    let (content_type, payload) = if status == 200 {
        let events = [
            json!({"type":"response.output_text.delta","item_id":"synthetic-message","content_index":0,"delta":ANSWER}),
            json!({"type":"response.completed","response":{"id":"synthetic-response","status":"completed","incomplete_details":null,"usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}}),
        ];
        (
            "text/event-stream",
            events
                .iter()
                .map(|event| format!("data: {event}\n\n"))
                .collect::<String>(),
        )
    } else {
        (
            "application/json",
            "{\"error\":{\"message\":\"synthetic rate limit\",\"type\":\"rate_limit_error\"}}"
                .to_owned(),
        )
    };
    let response = format!(
        "HTTP/1.1 {status} Synthetic\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    reader
        .get_mut()
        .write_all(response.as_bytes())
        .await
        .map_err(|_| "http response write")?;
    reader
        .get_mut()
        .shutdown()
        .await
        .map_err(|_| "http close")?;
    Ok(())
}

async fn run_case(status: u16, explicit_tool_off: bool, profile: ArtifactProfile) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let provider = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.map_err(|_| "http accept")?;
            if observed.fetch_add(1, Ordering::SeqCst) != 0 {
                return Err("unexpected provider retry");
            }
            timeout(
                Duration::from_secs(5),
                serve_one(stream, status, explicit_tool_off, profile),
            )
            .await
            .map_err(|_| "http timeout")??;
        }
        #[allow(unreachable_code)]
        Ok::<(), &'static str>(())
    });
    let architecture = if cfg!(target_arch = "x86_64") {
        LinuxArchitecture::Amd64
    } else {
        LinuxArchitecture::Arm64
    };
    let mut config = ProbeConfig::new(
        PathBuf::from("/scratch/pi"),
        PathBuf::from("/scratch"),
        architecture,
    );
    if profile == ArtifactProfile::NoTools1 {
        assert_eq!(architecture, LinuxArchitecture::Amd64);
        config.expected_digest = PATCHED_AMD64_SHA256.to_owned();
    }
    let prepared = artifact::prepare(&config).await.unwrap();
    if explicit_tool_off {
        let mut settings: Value =
            serde_json::from_slice(&tokio::fs::read(&prepared.settings).await.unwrap()).unwrap();
        settings["tools"] =
            json!({"loadMode":{"manage_skill":"off","xdev":"off","submit_plan":"off"}});
        tokio::fs::write(&prepared.settings, serde_json::to_vec(&settings).unwrap())
            .await
            .unwrap();
    }
    let models = json!({"providers":{"openai":{"baseUrl":format!("http://{address}/v1/responses"),"api":"openai-responses","modelOverrides":{"gpt-5.6-sol":{"maxTokens":800,"contextWindow":16384}}}}});
    tokio::fs::write(
        prepared.agent.join("models.json"),
        serde_json::to_vec(&models).unwrap(),
    )
    .await
    .unwrap();
    let mut command = process::isolated_command(&prepared);
    command
        .env("OPENAI_API_KEY", TOKEN)
        .args(["--system-prompt", SYSTEM]);
    let mut child = command.spawn().unwrap();
    let stderr = tokio::spawn(process::drain_stderr(child.stderr.take().unwrap()));
    let result = timeout(Duration::from_secs(15), async {
        let mut stdin = child.stdin.take().ok_or(Failure::ProcessIo)?;
        let mut stdout = BufReader::new(child.stdout.take().ok_or(Failure::ProcessIo)?);
        let mut total = 0;
        protocol::confirm_controls(&mut stdin, &mut stdout, &mut total, &mut Vec::new()).await?;
        let prompt = format!(
            "{}\n",
            json!({"type":"prompt","id":"synthetic-prompt","message":QUESTION})
        );
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(|_| Failure::ProcessIo)?;
        stdin.flush().await.map_err(|_| Failure::ProcessIo)?;
        let mut acknowledged = false;
        let mut provider_error = false;
        loop {
            let bytes = protocol::read_frame(&mut stdout, &mut total)
                .await?
                .ok_or(Failure::UnexpectedEof)?;
            let frame: Value =
                serde_json::from_slice(&bytes).map_err(|_| Failure::MalformedFrame)?;
            match frame["type"].as_str() {
                Some("response")
                    if frame["id"] == "synthetic-prompt"
                        && frame["command"] == "prompt"
                        && frame["success"] == true
                        && !acknowledged =>
                {
                    acknowledged = true
                }
                Some(
                    "agent_start" | "turn_start" | "message_start" | "message_update"
                    | "message_end" | "turn_end",
                ) => {}
                Some("provider_error") if status != 200 && !provider_error => provider_error = true,
                Some("agent_end") if acknowledged => {
                    if (status != 200) != provider_error || (status != 200) != frame["error"].is_string() {
                        return Err(Failure::UnsafeState);
                    }
                    break;
                }
                _ => {
                    println!("{}", json!({"synthetic_unexpected_event": frame["type"].as_str().filter(|name| name.len()<=64 && name.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'_')).unwrap_or("unrecognized")}));
                    return Err(Failure::UnexpectedFrame);
                }
            }
        }
        let request = b"{\"type\":\"get_last_assistant_text\",\"id\":\"synthetic-result\"}\n";
        stdin
            .write_all(request)
            .await
            .map_err(|_| Failure::ProcessIo)?;
        stdin.flush().await.map_err(|_| Failure::ProcessIo)?;
        let bytes = protocol::read_frame(&mut stdout, &mut total)
            .await?
            .ok_or(Failure::UnexpectedEof)?;
        let reply: Value = serde_json::from_slice(&bytes).map_err(|_| Failure::MalformedFrame)?;
        if reply["type"] != "response"
            || reply["id"] != "synthetic-result"
            || reply["success"] != true
        {
            return Err(Failure::UnexpectedFrame);
        }
        if status == 200 && reply["data"]["text"] != ANSWER {
            return Err(Failure::UnsafeState);
        }
        if status != 200
            && reply["data"]["text"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
        {
            return Err(Failure::UnsafeState);
        }
        drop(stdin);
        if protocol::read_frame(&mut stdout, &mut total)
            .await?
            .is_some()
        {
            return Err(Failure::UnexpectedFrame);
        }
        if !child
            .wait()
            .await
            .map_err(|_| Failure::ProcessIo)?
            .success()
        {
            return Err(Failure::ProcessExited);
        }
        Ok::<(), Failure>(())
    })
    .await;
    // Always kill/reap on failure before assertions; no orphan or private output.
    if !matches!(result, Ok(Ok(()))) {
        let _ = child.start_kill();
        let _ = timeout(Duration::from_secs(1), child.wait()).await;
    }
    let stderr_result = timeout(Duration::from_secs(1), stderr).await;
    let provider_result = if provider.is_finished() {
        Some(provider.await.unwrap())
    } else {
        provider.abort();
        let _ = provider.await;
        None
    };
    assert!(
        provider_result.is_none(),
        "fake provider contract: {provider_result:?}"
    );
    assert_eq!(result, Ok(Ok(())), "bounded synthetic Pi protocol");
    assert!(matches!(stderr_result, Ok(Ok(Ok(_)))));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "exactly one synthetic provider request"
    );
    assert!(crate::provider_execution_gate().is_err());
    println!(
        "{}",
        json!({"case_status":status,"explicit_tool_off":explicit_tool_off,"artifact_profile":profile.label(),"synthetic_provider_calls":1,"external_network":false,"real_credentials":false,"production_provider_gate":"closed","tool_free_gate":if profile == ArtifactProfile::NoTools1 {"request_schema_verified"} else {"blocked"}})
    );
}

#[tokio::test]
#[ignore = "requires the official pinned Pi artifact inside the network-none read-only-root sandbox"]
async fn sandboxed_real_fake_provider_exposes_unrequested_tool() {
    require_sandbox();
    run_case(200, false, ArtifactProfile::Official).await;
    run_case(200, true, ArtifactProfile::Official).await;
    run_case(429, true, ArtifactProfile::Official).await;
}

#[tokio::test]
#[ignore = "requires the separately pinned AMD64 no-tools.1 artifact inside the network-none read-only-root sandbox"]
async fn sandboxed_patched_fake_provider_has_no_tools() {
    require_sandbox();
    // A local patch must not silently pass as the official artifact.
    let official = ProbeConfig::new(
        PathBuf::from("/scratch/pi"),
        PathBuf::from("/scratch"),
        LinuxArchitecture::Amd64,
    );
    assert_eq!(
        artifact::prepare(&official).await.err(),
        Some(Failure::ArtifactDigestMismatch)
    );
    run_case(200, false, ArtifactProfile::NoTools1).await;
    run_case(200, true, ArtifactProfile::NoTools1).await;
    run_case(429, true, ArtifactProfile::NoTools1).await;
}

#[test]
fn patched_tool_schema_rejects_unexpected_or_malformed_declarations() {
    for body in [json!({}), json!({"tools":[]})] {
        assert_eq!(check_tools(&body, false, ArtifactProfile::NoTools1), Ok(()));
    }
    for body in [
        json!({"tools":[{"name":"xdev"}]}),
        json!({"tools":[{"name":"manage_skill"},{"name":"submit_plan"}]}),
        json!({"tools":null}),
        json!({"tools":{}}),
        json!({"tools":"off"}),
    ] {
        assert!(check_tools(&body, true, ArtifactProfile::NoTools1).is_err());
    }
    assert!(check_tools(&json!({}), false, ArtifactProfile::Official).is_err());
}

#[test]
fn local_patch_and_complete_upstream_notice_are_hash_bound() {
    use sha2::{Digest, Sha256};
    for (bytes, expected) in [
        (
            include_bytes!("../patches/no-tools-1.patch").as_slice(),
            "bec724b3d1cef1df4f4cddc50573ce6af1724c743496e7f5e900f4e4a6c815bc",
        ),
        (
            include_bytes!("../patches/LICENSE.pi").as_slice(),
            "32a82e0a5754e72e51fae44b65a936c831c07376f21c90f5fb9e76897fcc3509",
        ),
    ] {
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), expected);
    }
}
