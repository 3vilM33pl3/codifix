// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const MAX_FRAME_BYTES: usize = 16_384;
const MAX_STDOUT_BYTES: usize = 65_536;
const MAX_INPUT_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlCommand {
    DisableRetry,
    DisableCompaction,
    GetState,
}

impl ControlCommand {
    fn wire_name(self) -> &'static str {
        match self {
            Self::DisableRetry => "set_auto_retry",
            Self::DisableCompaction => "set_auto_compaction",
            Self::GetState => "get_state",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::DisableRetry => "control-1",
            Self::DisableCompaction => "control-2",
            Self::GetState => "control-3",
        }
    }

    fn encode(self) -> Result<Vec<u8>, Failure> {
        let mut frame = json!({"id": self.id(), "type": self.wire_name()});
        if self != Self::GetState {
            frame["enabled"] = json!(false);
        }
        let mut bytes = serde_json::to_vec(&frame).map_err(|_| Failure::MalformedFrame)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(Failure::InputLimit);
        }
        Ok(bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "event", content = "command")]
pub enum EventKind {
    Spawned,
    CommandSent(ControlCommand),
    ResponseAccepted(ControlCommand),
    TerminalError,
    RejectedFrame,
    StderrDiscarded,
    KillRequested,
    Reaped,
    ReapUnconfirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TranscriptEvent {
    pub kind: EventKind,
    pub bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupFailure {
    MissingCredential,
    InvalidCredential,
    ModelUnavailable,
    Configuration,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case", tag = "failure", content = "class")]
pub enum Failure {
    #[error("probe configuration is invalid")]
    InvalidConfiguration,
    #[error("the pinned artifact does not support this host architecture")]
    WrongArchitecture,
    #[error("pinned artifact is unavailable or not a bounded regular file")]
    ArtifactUnavailable,
    #[error("artifact does not match the pinned release SHA-256")]
    ArtifactDigestMismatch,
    #[error("scratch parent must be private, owned by this user, and available")]
    ScratchUnavailable,
    #[error("Pi process could not be started")]
    SpawnFailed,
    #[error("Pi startup failed: {0:?}")]
    Startup(StartupFailure),
    #[error("Pi reported a terminal runtime error")]
    TerminalError,
    #[error("Pi closed its protocol before the expected response")]
    UnexpectedEof,
    #[error("Pi exited unsuccessfully")]
    ProcessExited,
    #[error("Pi process I/O failed")]
    ProcessIo,
    #[error("Pi probe exceeded its deadline")]
    Deadline,
    #[error("Pi stdout/stderr exceeded the output bound")]
    OutputLimit,
    #[error("RPC request exceeded the input bound")]
    InputLimit,
    #[error("Pi emitted invalid JSONL or a malformed control response")]
    MalformedFrame,
    #[error("Pi emitted an unexpected or uncorrelated protocol frame")]
    UnexpectedFrame,
    #[error("Pi rejected a control command")]
    CommandRejected,
    #[error("Pi did not confirm the exact approved model and empty disabled state")]
    UnsafeState,
    #[error("the bounded process driver failed without a cleanup report")]
    DriverUnavailable,
    #[error(
        "provider execution is blocked until external per-request admission and isolation are verified"
    )]
    ProviderAdmissionUnavailable,
}

// Known fields use derived deserialization so duplicate safety-relevant fields,
// wrong types and missing state fields are rejected. Upstream diagnostic/model
// metadata is intentionally ignored and never copied into the public report.
#[derive(Deserialize)]
struct WireFrame {
    #[serde(rename = "type")]
    kind: String,
    id: Option<String>,
    command: Option<String>,
    success: Option<bool>,
    data: Option<WireState>,
    phase: Option<String>,
    code: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireState {
    model: WireModel,
    thinking_level: String,
    auto_retry_enabled: bool,
    auto_compaction_enabled: bool,
    is_streaming: bool,
    is_compacting: bool,
    message_count: u64,
    pending_message_count: u64,
    session_file: Value,
    session_name: Value,
}

#[derive(Deserialize)]
struct WireModel {
    provider: String,
    id: String,
}

pub(crate) async fn handshake(
    mut stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    transcript: &mut Vec<TranscriptEvent>,
) -> Result<(), Failure> {
    let mut reader = BufReader::new(stdout);
    let mut total = 0;
    confirm_controls(&mut stdin, &mut reader, &mut total, transcript).await?;
    drop(stdin);
    // A control-only probe must finish cleanly. Any subsequent frame (including
    // generation, tool or usage events) is unsolicited and makes it fail closed.
    if let Some(bytes) = read_frame(&mut reader, &mut total).await? {
        transcript.push(TranscriptEvent {
            kind: EventKind::RejectedFrame,
            bytes: bytes.len(),
        });
        return Err(Failure::UnexpectedFrame);
    }
    Ok(())
}

pub(crate) async fn confirm_controls(
    stdin: &mut tokio::process::ChildStdin,
    reader: &mut BufReader<tokio::process::ChildStdout>,
    total: &mut usize,
    transcript: &mut Vec<TranscriptEvent>,
) -> Result<(), Failure> {
    for command in [
        ControlCommand::DisableRetry,
        ControlCommand::DisableCompaction,
        ControlCommand::GetState,
    ] {
        let encoded = command.encode()?;
        stdin
            .write_all(&encoded)
            .await
            .map_err(|_| Failure::ProcessIo)?;
        stdin.flush().await.map_err(|_| Failure::ProcessIo)?;
        transcript.push(TranscriptEvent {
            kind: EventKind::CommandSent(command),
            bytes: encoded.len(),
        });
        let bytes = read_frame(reader, total)
            .await?
            .ok_or(Failure::UnexpectedEof)?;
        let response: WireFrame =
            serde_json::from_slice(&bytes).map_err(|_| Failure::MalformedFrame)?;
        match accept_response(command, response) {
            Ok(()) => transcript.push(TranscriptEvent {
                kind: EventKind::ResponseAccepted(command),
                bytes: bytes.len(),
            }),
            Err(failure) => {
                let kind = if matches!(failure, Failure::Startup(_) | Failure::TerminalError) {
                    EventKind::TerminalError
                } else {
                    EventKind::RejectedFrame
                };
                transcript.push(TranscriptEvent {
                    kind,
                    bytes: bytes.len(),
                });
                return Err(failure);
            }
        }
    }
    Ok(())
}

pub(crate) async fn read_frame(
    reader: &mut BufReader<tokio::process::ChildStdout>,
    total: &mut usize,
) -> Result<Option<Vec<u8>>, Failure> {
    read_frame_bounded(reader, total, MAX_FRAME_BYTES, MAX_STDOUT_BYTES).await
}

pub(crate) async fn read_frame_bounded<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    total: &mut usize,
    frame_limit: usize,
    total_limit: usize,
) -> Result<Option<Vec<u8>>, Failure> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(|_| Failure::ProcessIo)?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err(Failure::MalformedFrame)
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let length = newline.map_or(available.len(), |index| index + 1);
        if frame.len() + length > frame_limit || *total + length > total_limit {
            return Err(Failure::OutputLimit);
        }
        frame.extend_from_slice(&available[..length]);
        reader.consume(length);
        *total += length;
        if newline.is_some() {
            return Ok(Some(frame));
        }
    }
}

fn accept_response(command: ControlCommand, frame: WireFrame) -> Result<(), Failure> {
    if frame.kind == "error" {
        if frame.phase.as_deref() == Some("startup") {
            let class = match frame.code.as_deref() {
                Some("auth.missing_api_key") => StartupFailure::MissingCredential,
                Some("auth.invalid_api_key") => StartupFailure::InvalidCredential,
                Some("auth.no_models_available") => StartupFailure::ModelUnavailable,
                Some("config") => StartupFailure::Configuration,
                _ => StartupFailure::Other,
            };
            return Err(Failure::Startup(class));
        }
        return Err(Failure::TerminalError);
    }
    if frame.kind != "response"
        || frame.id.as_deref() != Some(command.id())
        || frame.command.as_deref() != Some(command.wire_name())
    {
        return Err(Failure::UnexpectedFrame);
    }
    if frame.success != Some(true) {
        return Err(Failure::CommandRejected);
    }
    if command == ControlCommand::GetState {
        let state = frame.data.ok_or(Failure::UnsafeState)?;
        if state.model.provider != "openai"
            || state.model.id != mem_supervisor::DEFAULT_MODEL
            || state.thinking_level != "off"
            || state.auto_retry_enabled
            || state.auto_compaction_enabled
            || state.is_streaming
            || state.is_compacting
            || state.message_count != 0
            || state.pending_message_count != 0
            || !state.session_file.is_null()
            || !state.session_name.is_null()
        {
            return Err(Failure::UnsafeState);
        }
    } else if frame.data.is_some() {
        return Err(Failure::UnexpectedFrame);
    }
    Ok(())
}
