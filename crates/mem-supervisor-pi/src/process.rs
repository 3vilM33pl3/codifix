// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{process::Stdio, time::Duration};

use tokio::{
    io::AsyncReadExt,
    process::Command,
    time::{Instant, timeout, timeout_at},
};

use crate::{
    EventKind, Failure, ProbeConfig, ProbeReport, ProcessDisposition, TranscriptEvent, artifact,
    control_arguments, protocol,
};

const MAX_STDERR_BYTES: usize = 8192;
const REAP_GRACE: Duration = Duration::from_secs(1);

pub(crate) async fn run(config: ProbeConfig) -> ProbeReport {
    let deadline = Instant::now() + config.deadline;
    let prepared = match timeout_at(deadline, artifact::prepare(&config)).await {
        Ok(Ok(prepared)) => prepared,
        Ok(Err(failure)) => return ProbeReport::failed(failure),
        Err(_) => return ProbeReport::failed(Failure::Deadline),
    };
    let mut command = isolated_command(&prepared);
    // Absent from production builds. This fixed, invalid non-secret token only
    // bypasses Pi's local presence check in the explicit network-none diagnostic.
    #[cfg(test)]
    if config.synthetic_auth_presence {
        command.env(
            "OPENAI_API_KEY",
            "synthetic-local-probe-not-a-valid-provider-key",
        );
    }
    let mut child = loop {
        if Instant::now() >= deadline {
            return ProbeReport::failed(Failure::Deadline);
        }
        match command.spawn() {
            Ok(child) => break child,
            // An unrelated concurrent fork can briefly inherit the copy's
            // CLOEXEC write handle before its own exec. No child was started on
            // ETXTBSY; retry only this OS condition within the original deadline.
            Err(error)
                if error.raw_os_error() == Some(rustix::io::Errno::TXTBSY.raw_os_error()) =>
            {
                if timeout_at(deadline, tokio::time::sleep(Duration::from_millis(10)))
                    .await
                    .is_err()
                {
                    return ProbeReport::failed(Failure::Deadline);
                }
            }
            Err(_) => return ProbeReport::failed(Failure::SpawnFailed),
        }
    };
    let mut transcript = vec![TranscriptEvent {
        kind: EventKind::Spawned,
        bytes: 0,
    }];
    // Child ownership lives outside the timed future. Timeout/error paths always
    // request a kill and explicitly await reaping before returning a report.
    let io = async {
        let stdin = child.stdin.take().ok_or(Failure::ProcessIo)?;
        let stdout = child.stdout.take().ok_or(Failure::ProcessIo)?;
        let stderr = child.stderr.take().ok_or(Failure::ProcessIo)?;
        let ((), stderr_bytes) = tokio::try_join!(
            protocol::handshake(stdin, stdout, &mut transcript),
            drain_stderr(stderr)
        )?;
        transcript.push(TranscriptEvent {
            kind: EventKind::StderrDiscarded,
            bytes: stderr_bytes,
        });
        let status = child.wait().await.map_err(|_| Failure::ProcessIo)?;
        if status.success() {
            Ok(())
        } else {
            Err(Failure::ProcessExited)
        }
    };
    let failure = match timeout_at(deadline, io).await {
        Ok(Ok(())) => None,
        Ok(Err(failure)) => Some(failure),
        Err(_) => Some(Failure::Deadline),
    };
    let process = if failure.is_none() {
        ProcessDisposition::Reaped
    } else {
        let _ = child.start_kill();
        transcript.push(TranscriptEvent {
            kind: EventKind::KillRequested,
            bytes: 0,
        });
        match timeout(REAP_GRACE, child.wait()).await {
            Ok(Ok(_)) => ProcessDisposition::Reaped,
            _ => ProcessDisposition::ReapUnconfirmed,
        }
    };
    transcript.push(TranscriptEvent {
        kind: if process == ProcessDisposition::Reaped {
            EventKind::Reaped
        } else {
            EventKind::ReapUnconfirmed
        },
        bytes: 0,
    });
    if process == ProcessDisposition::ReapUnconfirmed {
        // Do not remove a process's working files while its termination is
        // uncertain. The directory contains only fresh, credential-free data.
        let _ = prepared.scratch.keep();
    }
    ProbeReport {
        protocol_ready: failure.is_none() && process == ProcessDisposition::Reaped,
        provider_execution_enabled: false,
        failure,
        process,
        transcript,
    }
}

/// Process hygiene shared with the explicit fake-provider compatibility test.
/// This does not establish an OS sandbox or grant provider authority.
pub(crate) fn isolated_command(prepared: &artifact::Prepared) -> Command {
    let mut command = Command::new(&prepared.executable);
    command
        .args(control_arguments())
        .current_dir(&prepared.cwd)
        .env_clear()
        .env("HOME", &prepared.home)
        .env("XDG_CONFIG_HOME", prepared.home.join("config"))
        .env("XDG_DATA_HOME", prepared.home.join("data"))
        .env("XDG_STATE_HOME", prepared.home.join("state"))
        .env("XDG_CACHE_HOME", prepared.home.join("cache"))
        .env("XDG_RUNTIME_DIR", prepared.home.join("runtime"))
        .env("TMPDIR", prepared.scratch.path())
        .env("PI_CODING_AGENT_DIR", &prepared.agent)
        .env("PI_PACKAGE_DIR", prepared.agent.join("packages"))
        .env("PI_SESSIONS_DIR", prepared.agent.join("sessions"))
        .env("PI_CONFIG_PATH", &prepared.settings)
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C.UTF-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

pub(crate) async fn drain_stderr(
    mut stderr: tokio::process::ChildStderr,
) -> Result<usize, Failure> {
    let mut total = 0;
    let mut buffer = [0_u8; 1024];
    loop {
        let read = stderr
            .read(&mut buffer)
            .await
            .map_err(|_| Failure::ProcessIo)?;
        if read == 0 {
            return Ok(total);
        }
        total += read;
        if total > MAX_STDERR_BYTES {
            return Err(Failure::OutputLimit);
        }
        // Never retain or forward stderr text, even on a failed startup.
    }
}
