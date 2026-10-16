//! Native AppContainer driver. The trusted parent owns the job and filesystem
//! grants; the driver can inherit only its already admitted one-work broker.
use super::*;
use mem_platform::windows_process::{CodeFile, spawn_inherited};
use std::{
    ffi::OsString,
    os::windows::io::{BorrowedHandle, RawHandle},
};

const SETTINGS: &str = r#"{"retry":{"enabled":false,"maxRetries":0,"fallbackChains":{},"maxFailoversPerTurn":0},"compaction":{"enabled":false}}"#;

fn digest(value: &str) -> Result<[u8; 32], Failure> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Failure::InvalidConfiguration);
    }
    let mut result = [0; 32];
    for (index, output) in result.iter_mut().enumerate() {
        *output = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| Failure::InvalidConfiguration)?;
    }
    Ok(result)
}

pub async fn run_child() -> ChildReport {
    match execute().await {
        Ok(digest) => ChildReport {
            proposal_sha256: Some(digest),
            process: ProcessDisposition::Reaped,
            failure: None,
        },
        Err((reason, process)) => ChildReport {
            proposal_sha256: None,
            process,
            failure: Some(reason),
        },
    }
}

async fn execute() -> Result<[u8; 32], (Failure, ProcessDisposition)> {
    let not_started = |reason| (reason, ProcessDisposition::NotStarted);
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.len() != 2 || arguments[0] != "--windows-pi-sha256" {
        return Err(not_started(Failure::UnsafeState));
    }
    let sha256 = digest(&arguments[1]).map_err(not_started)?;
    let broker = std::env::var("MEMORY_BROKER_HANDLE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|raw| *raw != 0 && *raw != usize::MAX)
        .ok_or_else(|| not_started(Failure::UnsafeState))?;
    // Supplied by the protected parent STARTUPINFOEX handle list, never input
    // from the model or question. spawn_inherited validates its pipe type and
    // verifies the current/descendant AppContainer token before resuming Pi.
    let broker = unsafe { BorrowedHandle::borrow_raw(broker as RawHandle) };
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
    .await
    .map_err(|_| not_started(Failure::Deadline))?
    .map_err(not_started)?;
    let scratch = std::env::current_dir().map_err(|_| not_started(Failure::ScratchUnavailable))?;
    let agent = scratch.join("agent");
    std::fs::create_dir(&agent).map_err(|_| not_started(Failure::ScratchUnavailable))?;
    let settings = agent.join("settings.json");
    std::fs::write(&settings, SETTINGS).map_err(|_| not_started(Failure::ScratchUnavailable))?;
    let models = json!({"providers":{"openai":{"baseUrl":"http://memory-broker.invalid/v1/responses","api":"openai-responses","modelOverrides":{"gpt-5.6-sol":{"maxTokens":800,"contextWindow":16384}}}}});
    std::fs::write(agent.join("models.json"), models.to_string())
        .map_err(|_| not_started(Failure::ScratchUnavailable))?;
    let executable = std::env::current_exe()
        .map_err(|_| not_started(Failure::ArtifactUnavailable))?
        .parent()
        .ok_or_else(|| not_started(Failure::ArtifactUnavailable))?
        .join("pi.exe");
    let mut arguments: Vec<OsString> = crate::control_arguments()
        .iter()
        .map(|argument| {
            OsString::from(match *argument {
                "10" => "105",
                "5" => "100",
                other => other,
            })
        })
        .collect();
    arguments.extend([
        OsString::from("--system-prompt"),
        OsString::from(&input.system),
    ]);
    let overrides = [
        ("PI_CODING_AGENT_DIR".into(), agent.into_os_string()),
        ("PI_CONFIG_PATH".into(), settings.into_os_string()),
        (
            "PI_PACKAGE_DIR".into(),
            scratch.join("packages").into_os_string(),
        ),
        (
            "PI_SESSIONS_DIR".into(),
            scratch.join("sessions").into_os_string(),
        ),
        ("PI_CACHE_RETENTION".into(), "none".into()),
        ("OPENAI_API_KEY".into(), input.broker_token.into()),
    ];
    let mut child = spawn_inherited(
        CodeFile {
            path: executable,
            sha256,
        },
        &arguments,
        broker,
        &overrides,
    )
    .map_err(|_| not_started(Failure::SpawnFailed))?;
    let stdin = child
        .stdin
        .take()
        .ok_or((Failure::ProcessIo, ProcessDisposition::ReapUnconfirmed))?;
    let stdout = child
        .stdout
        .take()
        .ok_or((Failure::ProcessIo, ProcessDisposition::ReapUnconfirmed))?;
    let stderr = child
        .stderr
        .take()
        .ok_or((Failure::ProcessIo, ProcessDisposition::ReapUnconfirmed))?;
    let drain = tokio::spawn(async move {
        let mut bytes = Vec::new();
        tokio::fs::File::from_std(stderr)
            .take(8193)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| Failure::ProcessIo)?;
        if bytes.len() > 8192 {
            return Err(Failure::OutputLimit);
        }
        Ok::<_, Failure>(())
    });
    let result = timeout(DEADLINE, async {
        let result = exchange(
            tokio::fs::File::from_std(stdin),
            tokio::fs::File::from_std(stdout),
            &input.prompt,
        )
        .await?;
        loop {
            match child.try_wait().map_err(|_| Failure::ProcessIo)? {
                Some(0) => break,
                Some(_) => return Err(Failure::ProcessExited),
                None => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
        Ok(result)
    })
    .await
    .unwrap_or(Err(Failure::Deadline));
    if result.is_err() && child.terminate().is_err() {
        drain.abort();
        return Err((Failure::ProcessIo, ProcessDisposition::ReapUnconfirmed));
    }
    let stderr_ok = timeout(Duration::from_secs(1), drain).await;
    if !matches!(stderr_ok, Ok(Ok(Ok(())))) {
        return Err((Failure::OutputLimit, ProcessDisposition::Reaped));
    }
    result.map_err(|reason| (reason, ProcessDisposition::Reaped))
}
