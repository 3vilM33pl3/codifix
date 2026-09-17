// SPDX-License-Identifier: AGPL-3.0-or-later

//! Credential-free control probe for the pinned Pi Rust v0.5.1 RPC executable.
//!
//! No prompts, arbitrary commands, credential inputs or provider settings are
//! accepted. This is process/protocol hygiene, not an OS or network sandbox.
//! Provider execution remains blocked after a successful probe. Separate ignored
//! diagnostics exercise authentic Pi only inside an independently enforced
//! network-disabled container; normal tests use fake processes. Missing local
//! credential presence prevents a production control handshake.

mod artifact;
mod process;
mod protocol;

use std::{path::PathBuf, time::Duration};

use serde::Serialize;

pub use protocol::{ControlCommand, EventKind, Failure, StartupFailure, TranscriptEvent};

pub const PI_VERSION: &str = "0.5.1";
pub const PI_SOURCE_REVISION: &str = "7c018475d54d4e09c907691e47a520f1f09e3288";
pub const AMD64_BINARY_SHA256: &str =
    "07c13337ab52695d3d8897e37fac7164ce97184b3c4ecc6ce01b4ea0ceef6677";
pub const ARM64_BINARY_SHA256: &str =
    "0190009efe002c3cbca80a5f005472cea0df5901706c9e49dcb6c4c6891e0884";
pub const AMD64_ARCHIVE_SHA256: &str =
    "13c39f3e9a45c2f6da0b1069521cda5a4ae4e99c29ec91422231b508dcfe3f0c";
pub const ARM64_ARCHIVE_SHA256: &str =
    "89906e3fd11842c1a571f174740a188ebdc6c8a7a65f51f1d6f0b23800ad2e44";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxArchitecture {
    Amd64,
    Arm64,
}

/// Only paths to an operator-supplied pinned binary and private scratch parent
/// are accepted. Neither custom digests nor extra argv/environment are public.
#[derive(Clone)]
pub struct ProbeConfig {
    executable: PathBuf,
    scratch_parent: PathBuf,
    expected_digest: String,
    deadline: Duration,
    architecture: LinuxArchitecture,
    #[cfg(test)]
    synthetic_auth_presence: bool,
}

impl ProbeConfig {
    pub fn new(
        executable: PathBuf,
        scratch_parent: PathBuf,
        architecture: LinuxArchitecture,
    ) -> Self {
        let expected_digest = match architecture {
            LinuxArchitecture::Amd64 => AMD64_BINARY_SHA256,
            LinuxArchitecture::Arm64 => ARM64_BINARY_SHA256,
        };
        Self {
            executable,
            scratch_parent,
            expected_digest: expected_digest.to_owned(),
            deadline: Duration::from_secs(10),
            architecture,
            #[cfg(test)]
            synthetic_auth_presence: false,
        }
    }

    /// One preparation/handshake/exit deadline, followed by at most one second
    /// of explicit kill-and-reap cleanup. Individual commands never reset it.
    pub fn with_deadline(mut self, deadline: Duration) -> Result<Self, Failure> {
        if deadline.is_zero() || deadline > Duration::from_secs(30) {
            return Err(Failure::InvalidConfiguration);
        }
        self.deadline = deadline;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessDisposition {
    NotStarted,
    Reaped,
    ReapUnconfirmed,
}

/// This report deliberately has no free-form Pi text, local paths, PID, provider
/// payload, credentials, reasoning, response body, usage or prompt fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProbeReport {
    pub protocol_ready: bool,
    pub provider_execution_enabled: bool,
    pub failure: Option<Failure>,
    pub process: ProcessDisposition,
    pub transcript: Vec<TranscriptEvent>,
}

impl ProbeReport {
    fn failed(failure: Failure) -> Self {
        Self {
            protocol_ready: false,
            provider_execution_enabled: false,
            failure: Some(failure),
            process: ProcessDisposition::NotStarted,
            transcript: Vec::new(),
        }
    }
}

/// A control handshake cannot open the external per-provider-request gate.
pub fn provider_execution_gate() -> Result<(), Failure> {
    Err(Failure::ProviderAdmissionUnavailable)
}

/// Execute three fixed control commands only, then close stdin and require EOF.
///
/// Requires a Tokio runtime. The bounded driver owns the child in a separate
/// task, so cancelling the caller's wait does not cancel cleanup. Runtime shutdown
/// still relies on Tokio's kill-on-drop/reaper fallback and provides no report.
pub async fn probe(config: &ProbeConfig) -> ProbeReport {
    match tokio::spawn(process::run(config.clone())).await {
        Ok(report) => report,
        Err(_) => {
            let mut report = ProbeReport::failed(Failure::DriverUnavailable);
            report.process = ProcessDisposition::ReapUnconfirmed;
            report
        }
    }
}

/// Every entry is a separate OS argument. No shell, API key, prompt, continuation,
/// resource path, model fallback or additional tool is supplied to Pi.
pub fn control_arguments() -> &'static [&'static str] {
    &[
        "--mode",
        "rpc",
        "--provider",
        "openai",
        "--model",
        mem_supervisor::DEFAULT_MODEL,
        "--thinking",
        "off",
        "--no-tools",
        "--no-extensions",
        "--no-skills",
        "--no-context-files",
        "--no-prompt-templates",
        "--no-themes",
        "--no-session",
        "--no-migrations",
        "--max-tool-iterations",
        "1",
        "--max-time",
        "10",
        "--request-timeout",
        "5",
        "--hide-cwd-in-prompt",
    ]
}

#[cfg(test)]
mod tests;
