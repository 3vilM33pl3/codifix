// SPDX-License-Identifier: AGPL-3.0-or-later
//! Narrow child driver for the separately pinned no-tools.1 build. It can reach
//! only a fixed private Unix broker through a bounded loopback relay. The trusted
//! parent MUST independently enforce the container, authorize the exact input,
//! and compare this untrusted child report with its durable proposal ledger.
use crate::{Failure, ProcessDisposition, protocol};
#[cfg(unix)]
use crate::{LinuxArchitecture, ProbeConfig, artifact, process};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;
#[cfg(unix)]
use std::{
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::PathBuf,
};
#[cfg(unix)]
use tokio::net::{TcpListener, UnixStream};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    time::timeout,
};
#[cfg(windows)]
#[path = "sandbox_windows.rs"]
mod windows;
#[cfg(windows)]
pub use windows::run_child;

pub const PATCHED_AMD64_SHA256: &str =
    "912366d4178bd7c12ccf4401b6241b1dcb6f8cdbd66302bc1764a1cec9dfe88c";
pub const PATCHED_ARM64_SHA256: &str =
    "c2d51cd7b0683c85477eed8c03be3562fc0d89a9915f085c0608740b8704363b";
#[cfg(unix)]
const SOCKET: &str = "/broker/job.sock";
const MAX_INPUT: u64 = 256 * 1024;
const DEADLINE: Duration = Duration::from_secs(110);

/// Sensitive stdin-only contract, deliberately no Debug. Never provider/DB keys,
/// endpoints, filesystem paths, continuation state or caller CLI arguments.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChildInput {
    pub system: String,
    pub prompt: String,
    pub broker_token: String,
}
impl ChildInput {
    fn validate(&self) -> Result<(), Failure> {
        if self.system.trim().is_empty()
            || self.system.len() > 8192
            || self.system.contains('\0')
            || self.prompt.trim().is_empty()
            || self.prompt.len() > 224 * 1024
            || self.broker_token.len() != 64
            || !self.broker_token.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Failure::InvalidConfiguration);
        }
        Ok(())
    }
}

/// Digest confirms byte delivery only, not correctness, authority or billing.
/// Never treat successful child metadata as sufficient to complete an action.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildReport {
    pub proposal_sha256: Option<[u8; 32]>,
    pub process: ProcessDisposition,
    pub failure: Option<Failure>,
}

/// Defensive checks catch accidental host invocation. They are not a proof of
/// independent isolation: the parent supplies network-none/read-only/mount rules.
#[cfg(unix)]
fn check_boundary() -> Result<(), Failure> {
    let uid = rustix::process::geteuid().as_raw();
    if uid == 0 || std::env::args_os().count() != 1 {
        return Err(Failure::UnsafeState);
    }
    let interfaces = std::fs::read_dir("/sys/class/net").map_err(|_| Failure::UnsafeState)?;
    let names = interfaces
        .map(|entry| entry.map(|e| e.file_name()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|_| Failure::UnsafeState)?;
    if names != ["lo"] {
        return Err(Failure::UnsafeState);
    }
    let status = std::fs::read_to_string("/proc/self/status").map_err(|_| Failure::UnsafeState)?;
    if !status.lines().any(|line| line == "NoNewPrivs:\t1")
        || !status
            .lines()
            .any(|line| line == "CapEff:\t0000000000000000")
    {
        return Err(Failure::UnsafeState);
    }
    let mounts = std::fs::read_to_string("/proc/mounts").map_err(|_| Failure::UnsafeState)?;
    if !mounts.lines().any(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        fields.len() >= 4 && fields[1] == "/" && fields[3].split(',').any(|option| option == "ro")
    }) {
        return Err(Failure::UnsafeState);
    }
    let directory = std::fs::symlink_metadata("/broker").map_err(|_| Failure::UnsafeState)?;
    let socket = std::fs::symlink_metadata(SOCKET).map_err(|_| Failure::UnsafeState)?;
    if !directory.is_dir()
        || directory.uid() != uid
        || directory.mode() & 0o777 != 0o700
        || !socket.file_type().is_socket()
        || socket.uid() != uid
        || socket.mode() & 0o777 != 0o600
    {
        return Err(Failure::UnsafeState);
    }
    Ok(())
}

#[cfg(unix)]
pub async fn run_child() -> ChildReport {
    let failure = |reason| ChildReport {
        proposal_sha256: None,
        process: ProcessDisposition::NotStarted,
        failure: Some(reason),
    };
    if let Err(reason) = check_boundary() {
        return failure(reason);
    }
    let input = timeout(Duration::from_secs(2), async {
        let mut bytes = Vec::new();
        tokio::io::stdin()
            .take(MAX_INPUT + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| Failure::ProcessIo)?;
        if bytes.len() as u64 > MAX_INPUT {
            return Err(Failure::InputLimit);
        }
        let input: ChildInput =
            serde_json::from_slice(&bytes).map_err(|_| Failure::MalformedFrame)?;
        input.validate()?;
        Ok::<_, Failure>(input)
    })
    .await;
    let input = match input {
        Ok(Ok(input)) => input,
        Ok(Err(reason)) => return failure(reason),
        Err(_) => return failure(Failure::Deadline),
    };
    let architecture = if cfg!(target_arch = "x86_64") {
        LinuxArchitecture::Amd64
    } else {
        LinuxArchitecture::Arm64
    };
    let mut config = ProbeConfig::new(
        PathBuf::from("/pi/pi"),
        PathBuf::from("/scratch"),
        architecture,
    );
    config.expected_digest = match architecture {
        LinuxArchitecture::Amd64 => PATCHED_AMD64_SHA256,
        LinuxArchitecture::Arm64 => PATCHED_ARM64_SHA256,
    }
    .to_owned();
    let prepared = match timeout(Duration::from_secs(10), artifact::prepare(&config)).await {
        Ok(Ok(prepared)) => prepared,
        Ok(Err(reason)) => return failure(reason),
        Err(_) => return failure(Failure::Deadline),
    };
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(l) => l,
        Err(_) => return failure(Failure::ProcessIo),
    };
    let address = match listener.local_addr() {
        Ok(a) => a,
        Err(_) => return failure(Failure::ProcessIo),
    };
    let models = json!({"providers":{"openai":{"baseUrl":format!("http://{address}/v1/responses"),"api":"openai-responses","modelOverrides":{"gpt-5.6-sol":{"maxTokens":800,"contextWindow":16384}}}}});
    if tokio::fs::write(prepared.agent.join("models.json"), models.to_string())
        .await
        .is_err()
    {
        return failure(Failure::ScratchUnavailable);
    }
    let relay = tokio::spawn(async move { timeout(DEADLINE, relay_one(listener)).await });
    let arguments: Vec<_> = crate::control_arguments()
        .iter()
        .map(|argument| match *argument {
            "10" => "105",
            "5" => "100",
            other => other,
        })
        .collect();
    let mut command = process::isolated_command_with_arguments(&prepared, &arguments);
    command
        .env("OPENAI_API_KEY", &input.broker_token)
        .env("PI_CACHE_RETENTION", "none")
        .args(["--system-prompt", &input.system]);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            relay.abort();
            return failure(Failure::SpawnFailed);
        }
    };
    let stderr = child
        .stderr
        .take()
        .map(|stderr| tokio::spawn(process::drain_stderr(stderr)));
    let result = timeout(DEADLINE, async {
        let stdin = child.stdin.take().ok_or(Failure::ProcessIo)?;
        let stdout = child.stdout.take().ok_or(Failure::ProcessIo)?;
        let digest = exchange(stdin, stdout, &input.prompt).await?;
        if !child
            .wait()
            .await
            .map_err(|_| Failure::ProcessIo)?
            .success()
        {
            return Err(Failure::ProcessExited);
        }
        Ok::<_, Failure>(digest)
    })
    .await;
    let (digest, mut error) = match result {
        Ok(Ok(d)) => (Some(d), None),
        Ok(Err(e)) => (None, Some(e)),
        Err(_) => (None, Some(Failure::Deadline)),
    };
    let process = if error.is_none() {
        ProcessDisposition::Reaped
    } else {
        let _ = child.start_kill();
        match timeout(Duration::from_secs(1), child.wait()).await {
            Ok(Ok(_)) => ProcessDisposition::Reaped,
            _ => ProcessDisposition::ReapUnconfirmed,
        }
    };
    if let Some(mut stderr) = stderr {
        if !matches!(
            timeout(Duration::from_secs(1), &mut stderr).await,
            Ok(Ok(Ok(_)))
        ) {
            stderr.abort();
            error.get_or_insert(Failure::OutputLimit);
        }
    } else {
        error.get_or_insert(Failure::ProcessIo);
    }
    // HTTP keep-alive can leave the relay open after Pi exits; aborting closes
    // both handles. Broker-owned durable work is independent of this connection.
    if relay.is_finished() {
        if !matches!(relay.await, Ok(Ok(Ok(())))) {
            error.get_or_insert(Failure::ProcessIo);
        }
    } else {
        relay.abort();
        let _ = relay.await;
    }
    if process == ProcessDisposition::ReapUnconfirmed {
        let _ = prepared.scratch.keep();
    }
    ChildReport {
        proposal_sha256: if error.is_none() { digest } else { None },
        process,
        failure: error,
    }
}

#[cfg(unix)]
async fn relay_one(listener: TcpListener) -> Result<(), Failure> {
    let (tcp, _) = listener.accept().await.map_err(|_| Failure::ProcessIo)?;
    drop(listener); // exactly one upstream connection; no retry/fallback channel
    let unix = UnixStream::connect(SOCKET)
        .await
        .map_err(|_| Failure::ProcessIo)?;
    let (tcp_read, tcp_write) = tcp.into_split();
    let (unix_read, unix_write) = unix.into_split();
    tokio::try_join!(
        copy_bounded(tcp_read, unix_write, 512 * 1024),
        copy_bounded(unix_read, tcp_write, 128 * 1024)
    )?;
    Ok(())
}

#[cfg(any(unix, test))]
async fn copy_bounded<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut read: R,
    mut write: W,
    limit: usize,
) -> Result<(), Failure> {
    let mut total = 0usize;
    let mut bytes = [0u8; 8192];
    loop {
        let size = read
            .read(&mut bytes)
            .await
            .map_err(|_| Failure::ProcessIo)?;
        if size == 0 {
            return write.shutdown().await.map_err(|_| Failure::ProcessIo);
        }
        total += size;
        if total > limit {
            return Err(Failure::OutputLimit);
        }
        write
            .write_all(&bytes[..size])
            .await
            .map_err(|_| Failure::ProcessIo)?;
    }
}

#[derive(Deserialize)]
struct Frame {
    #[serde(rename = "type")]
    kind: String,
    id: Option<String>,
    command: Option<String>,
    success: Option<bool>,
    error: Option<Value>,
    data: Option<Text>,
}
#[derive(Deserialize)]
struct Text {
    text: Option<String>,
}

async fn exchange<W: AsyncWrite + Unpin, R: AsyncRead + Unpin>(
    mut stdin: W,
    stdout: R,
    prompt: &str,
) -> Result<[u8; 32], Failure> {
    let mut stdout = BufReader::new(stdout);
    let mut total = 0;
    protocol::confirm_controls(&mut stdin, &mut stdout, &mut total, &mut Vec::new()).await?;
    let message = format!(
        "{}\n",
        json!({"type":"prompt","id":"broker-prompt","message":prompt})
    );
    stdin
        .write_all(message.as_bytes())
        .await
        .map_err(|_| Failure::ProcessIo)?;
    stdin.flush().await.map_err(|_| Failure::ProcessIo)?;
    let mut acknowledged = false;
    loop {
        let bytes =
            protocol::read_frame_bounded(&mut stdout, &mut total, 256 * 1024, 2 * 1024 * 1024)
                .await?
                .ok_or(Failure::UnexpectedEof)?;
        let frame: Frame = serde_json::from_slice(&bytes).map_err(|_| Failure::MalformedFrame)?;
        match frame.kind.as_str() {
            "response"
                if frame.id.as_deref() == Some("broker-prompt")
                    && frame.command.as_deref() == Some("prompt")
                    && frame.success == Some(true)
                    && !acknowledged =>
            {
                acknowledged = true
            }
            "agent_start" | "turn_start" | "message_start" | "message_update" | "message_end"
            | "turn_end" => {}
            "agent_end" if acknowledged && frame.error.as_ref().is_none_or(Value::is_null) => break,
            "provider_error" | "error" => return Err(Failure::TerminalError),
            _ => return Err(Failure::UnexpectedFrame),
        }
    }
    stdin
        .write_all(b"{\"type\":\"get_last_assistant_text\",\"id\":\"broker-result\"}\n")
        .await
        .map_err(|_| Failure::ProcessIo)?;
    stdin.flush().await.map_err(|_| Failure::ProcessIo)?;
    let bytes = protocol::read_frame_bounded(&mut stdout, &mut total, 256 * 1024, 2 * 1024 * 1024)
        .await?
        .ok_or(Failure::UnexpectedEof)?;
    let frame: Frame = serde_json::from_slice(&bytes).map_err(|_| Failure::MalformedFrame)?;
    if frame.kind != "response"
        || frame.id.as_deref() != Some("broker-result")
        || frame.command.as_deref() != Some("get_last_assistant_text")
        || frame.success != Some(true)
    {
        return Err(Failure::UnexpectedFrame);
    }
    let text = frame
        .data
        .and_then(|data| data.text)
        .filter(|text| !text.is_empty() && text.len() <= 65_536)
        .ok_or(Failure::OutputLimit)?;
    let digest = Sha256::digest(text.as_bytes()).into();
    drop(stdin);
    if protocol::read_frame_bounded(&mut stdout, &mut total, 256 * 1024, 2 * 1024 * 1024)
        .await?
        .is_some()
    {
        return Err(Failure::UnexpectedFrame);
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_input_is_bounded_and_cannot_override_paths_or_endpoints() {
        let valid =
            json!({"system":"Synthetic", "prompt":"Synthetic", "broker_token":"a".repeat(64)});
        let input: ChildInput = serde_json::from_value(valid.clone()).expect("input");
        assert!(input.validate().is_ok());
        let mut changed = valid.clone();
        changed["url"] = json!("https://example.invalid");
        assert!(serde_json::from_value::<ChildInput>(changed).is_err());
        for (key, value) in [
            ("system", "a".repeat(8193)),
            ("system", "\0".into()),
            ("prompt", " ".into()),
            ("prompt", "a".repeat(224 * 1024 + 1)),
            ("broker_token", "not-a-provider-key".into()),
        ] {
            let mut changed = valid.clone();
            changed[key] = json!(value);
            assert!(
                serde_json::from_value::<ChildInput>(changed)
                    .expect("shape")
                    .validate()
                    .is_err()
            );
        }
        assert!(
            serde_json::from_str::<ChildInput>(
                r#"{"system":"a","system":"b","prompt":"p","broker_token":"a"}"#
            )
            .is_err()
        );
    }
    #[tokio::test]
    async fn relay_bounds_both_payload_directions() {
        let (mut client, read) = tokio::io::duplex(16);
        let (write, mut server) = tokio::io::duplex(16);
        client.write_all(b"oversized").await.expect("write");
        drop(client);
        assert_eq!(
            copy_bounded(read, write, 3).await,
            Err(Failure::OutputLimit)
        );
        let mut output = Vec::new();
        server.read_to_end(&mut output).await.expect("read");
        assert!(output.is_empty());
    }
}
