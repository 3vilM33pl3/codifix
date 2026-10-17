// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    time::Duration,
};

use sha2::{Digest, Sha256};
use tokio::time::{Instant, sleep};

use crate::{
    ControlCommand, EventKind, Failure, LinuxArchitecture, ProbeConfig, ProcessDisposition,
    StartupFailure, probe,
};

struct Fixture {
    _root: tempfile::TempDir,
    config: ProbeConfig,
    pid_file: PathBuf,
}

impl Fixture {
    fn new(mode: &str) -> Self {
        let root = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let scratch = root.path().join("scratch");
        fs::create_dir(&scratch).unwrap();
        fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700)).unwrap();
        let executable = root.path().join("fixture-pi");
        let pid_file = root.path().join("pid");
        let source = format!(
            "#!/usr/bin/python3\nMODE={}\nPID_FILE={}\n{}",
            serde_json::to_string(mode).unwrap(),
            serde_json::to_string(&pid_file).unwrap(),
            include_str!("fake_pi.py")
        );
        fs::write(&executable, source.as_bytes()).unwrap();
        let architecture = if cfg!(target_arch = "x86_64") {
            LinuxArchitecture::Amd64
        } else {
            LinuxArchitecture::Arm64
        };
        let mut config = ProbeConfig::new(executable, scratch, architecture)
            .with_deadline(Duration::from_secs(3))
            .unwrap();
        // This override exists only inside the crate's private test module.
        // Production callers cannot provide their own executable digest.
        config.expected_digest = format!("{:x}", Sha256::digest(source.as_bytes()));
        Self {
            _root: root,
            config,
            pid_file,
        }
    }

    fn assert_reaped(&self) {
        let pid = fs::read_to_string(&self.pid_file).unwrap();
        assert!(
            !Path::new("/proc").join(pid).exists(),
            "child still present after explicit reap"
        );
        assert_eq!(
            fs::read_dir(&self.config.scratch_parent).unwrap().count(),
            0
        );
    }
}

#[tokio::test]
async fn fixed_handshake_confirms_state_without_opening_provider_gate() {
    let fixture = Fixture::new("happy");
    let report = probe(&fixture.config).await;
    assert!(report.protocol_ready, "{report:?}");
    assert_eq!(report.failure, None);
    assert_eq!(report.process, ProcessDisposition::Reaped);
    assert!(!report.provider_execution_enabled);
    assert_eq!(
        crate::provider_execution_gate(),
        Err(Failure::ProviderAdmissionUnavailable)
    );
    let accepted: Vec<_> = report
        .transcript
        .iter()
        .filter_map(|entry| match entry.kind {
            EventKind::ResponseAccepted(command) => Some(command),
            _ => None,
        })
        .collect();
    assert_eq!(
        accepted,
        [
            ControlCommand::DisableRetry,
            ControlCommand::DisableCompaction,
            ControlCommand::GetState
        ]
    );
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("DO_NOT_LEAK"));
    assert!(!json.contains(fixture._root.path().to_str().unwrap()));
    fixture.assert_reaped();
}

#[tokio::test]
async fn protocol_failures_are_bounded_redacted_and_reaped() {
    for (mode, expected) in [
        ("malformed", Failure::MalformedFrame),
        ("invalid_utf8", Failure::MalformedFrame),
        ("unterminated", Failure::MalformedFrame),
        ("duplicate", Failure::MalformedFrame),
        ("uncorrelated", Failure::UnexpectedFrame),
        ("wrong_command", Failure::UnexpectedFrame),
        ("unrequested_event", Failure::UnexpectedFrame),
        ("replayed", Failure::UnexpectedFrame),
        ("rejected", Failure::CommandRejected),
        ("eof", Failure::UnexpectedEof),
        (
            "startup_error",
            Failure::Startup(StartupFailure::MissingCredential),
        ),
        ("terminal_error", Failure::TerminalError),
        ("missing_state", Failure::MalformedFrame),
        ("trailing_event", Failure::UnexpectedFrame),
        ("exit_failure", Failure::ProcessExited),
        ("stdout_limit", Failure::OutputLimit),
        ("stderr_limit", Failure::OutputLimit),
    ] {
        let fixture = Fixture::new(mode);
        let report = probe(&fixture.config).await;
        assert_eq!(report.failure, Some(expected), "mode {mode}: {report:?}");
        assert!(!report.protocol_ready);
        assert!(!report.provider_execution_enabled);
        assert_eq!(report.process, ProcessDisposition::Reaped, "mode {mode}");
        assert!(report.transcript.len() <= 12);
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("DO_NOT_LEAK")
        );
        fixture.assert_reaped();
    }
}

#[tokio::test]
async fn exact_model_and_empty_disabled_state_are_required() {
    for mode in [
        "wrong_model",
        "wrong_provider",
        "thinking_enabled",
        "retry_enabled",
        "compaction_enabled",
        "streaming",
        "compacting",
        "context_loaded",
        "pending",
        "session_file",
        "session_name",
    ] {
        let fixture = Fixture::new(mode);
        let report = probe(&fixture.config).await;
        assert_eq!(
            report.failure,
            Some(Failure::UnsafeState),
            "{mode}: {report:?}"
        );
        fixture.assert_reaped();
    }
}

#[tokio::test]
async fn one_deadline_covers_handshake_and_process_exit_then_kills_and_reaps() {
    for mode in [
        "timeout_before_response",
        "slow_handshake",
        "linger_after_eof",
    ] {
        let mut fixture = Fixture::new(mode);
        // Leave room for parallel test startup/filesystem scheduling. The fake
        // still delays the three-command handshake beyond this shared deadline.
        fixture.config.deadline = Duration::from_millis(1500);
        let started = Instant::now();
        let report = probe(&fixture.config).await;
        assert_eq!(
            report.failure,
            Some(Failure::Deadline),
            "{mode}: {report:?}"
        );
        assert_eq!(report.process, ProcessDisposition::Reaped);
        assert!(started.elapsed() < Duration::from_secs(3));
        fixture.assert_reaped();
    }
}

#[tokio::test]
async fn cancelling_the_waiter_keeps_the_bounded_cleanup_owner_alive() {
    let mut fixture = Fixture::new("timeout_before_response");
    fixture.config.deadline = Duration::from_secs(2);
    let config = fixture.config.clone();
    let waiting = tokio::spawn(async move { probe(&config).await });
    for _ in 0..300 {
        if fixture.pid_file.exists() {
            break;
        }
        sleep(Duration::from_millis(5)).await;
    }
    assert!(fixture.pid_file.exists());
    waiting.abort();
    let _ = waiting.await;
    sleep(Duration::from_millis(2200)).await;
    fixture.assert_reaped();
}

#[tokio::test]
async fn missing_or_wrong_artifact_never_starts_a_process() {
    let mut fixture = Fixture::new("happy");
    fixture.config.expected_digest = "0".repeat(64);
    let report = probe(&fixture.config).await;
    assert_eq!(report.failure, Some(Failure::ArtifactDigestMismatch));
    assert_eq!(report.process, ProcessDisposition::NotStarted);
    assert!(!fixture.pid_file.exists());
    fs::write(&fixture.config.executable, []).unwrap();
    let report = probe(&fixture.config).await;
    assert_eq!(report.failure, Some(Failure::ArtifactUnavailable));
}

#[tokio::test]
async fn symlinks_and_shared_scratch_are_rejected_before_spawn() {
    let mut fixture = Fixture::new("happy");
    let original = fixture.config.executable.clone();
    let link = fixture._root.path().join("symlink-pi");
    symlink(&original, &link).unwrap();
    fixture.config.executable = link;
    assert_eq!(
        probe(&fixture.config).await.failure,
        Some(Failure::ArtifactUnavailable)
    );
    let link_parent = fixture._root.path().join("symlink-parent");
    symlink(fixture._root.path(), &link_parent).unwrap();
    fixture.config.executable = link_parent.join("fixture-pi");
    assert_eq!(
        probe(&fixture.config).await.failure,
        Some(Failure::ArtifactUnavailable)
    );
    fixture.config.executable = original;
    fs::set_permissions(
        &fixture.config.scratch_parent,
        fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    assert_eq!(
        probe(&fixture.config).await.failure,
        Some(Failure::ScratchUnavailable)
    );
    assert!(!fixture.pid_file.exists());
}

#[tokio::test]
async fn unsupported_architecture_and_deadline_are_rejected() {
    let mut fixture = Fixture::new("happy");
    fixture.config.architecture = if cfg!(target_arch = "x86_64") {
        LinuxArchitecture::Arm64
    } else {
        LinuxArchitecture::Amd64
    };
    assert_eq!(
        probe(&fixture.config).await.failure,
        Some(Failure::WrongArchitecture)
    );
    assert!(matches!(
        fixture.config.clone().with_deadline(Duration::ZERO),
        Err(Failure::InvalidConfiguration)
    ));
    assert!(matches!(
        fixture.config.with_deadline(Duration::from_secs(31)),
        Err(Failure::InvalidConfiguration)
    ));
    assert!(!fixture.pid_file.exists());
}

#[tokio::test]
async fn private_scratch_beneath_shared_nonsticky_ancestor_is_rejected() {
    let fixture = Fixture::new("happy");
    fs::set_permissions(fixture._root.path(), fs::Permissions::from_mode(0o777)).unwrap();
    let report = probe(&fixture.config).await;
    assert_eq!(report.failure, Some(Failure::ScratchUnavailable));
    assert_eq!(report.process, ProcessDisposition::NotStarted);
    assert!(!fixture.pid_file.exists());
    assert_eq!(
        fs::read_dir(&fixture.config.scratch_parent)
            .unwrap()
            .count(),
        0
    );
    fs::set_permissions(fixture._root.path(), fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn parent_credentials_and_loader_overrides_are_not_inherited() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::nested_environment_probe", "--nocapture"])
        .env("MEMORY_PI_NESTED_ENVIRONMENT_TEST", "1")
        .env("OPENAI_API_KEY", "synthetic-private-key")
        .env(
            "MEMORY_LAYER__DATABASE__URL",
            "postgresql://synthetic:secret@invalid/never",
        )
        .env("PI_CODING_AGENT_DIR", "/not-the-isolated-agent-dir")
        .env("PYTHONPATH", "/not-an-approved-resource")
        .env("HTTPS_PROXY", "http://never-contact.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-private-key"));
}

#[tokio::test]
async fn nested_environment_probe() {
    if std::env::var_os("MEMORY_PI_NESTED_ENVIRONMENT_TEST").is_none() {
        return;
    }
    let fixture = Fixture::new("happy");
    let report = probe(&fixture.config).await;
    assert!(report.protocol_ready, "{report:?}");
    fixture.assert_reaped();
}

/// An explicit operator-run diagnostic, never part of the fake-process suite.
/// The marker records caller intent; it cannot itself prove container isolation.
#[tokio::test]
#[ignore = "requires an independently prepared network-none/read-only container and official pinned artifact"]
async fn sandboxed_real_credential_free_probe() {
    assert_eq!(
        std::env::var("MEMORY_PI_SANDBOXED_PROBE").unwrap(),
        "network-none-read-only-root"
    );
    let architecture = if cfg!(target_arch = "x86_64") {
        LinuxArchitecture::Amd64
    } else {
        LinuxArchitecture::Arm64
    };
    let config = ProbeConfig::new(
        PathBuf::from("/scratch/pi"),
        PathBuf::from("/scratch"),
        architecture,
    );
    let report = probe(&config).await;
    println!("{}", serde_json::to_string(&report).unwrap());
    // The authentic pinned binary requires auth before it accepts control RPCs.
    // This proves blocked startup and cleanup, not model/handshake readiness.
    assert_eq!(
        report.failure,
        Some(Failure::Startup(StartupFailure::MissingCredential))
    );
    assert!(!report.protocol_ready);
    assert!(!report.provider_execution_enabled);
    assert_eq!(report.process, ProcessDisposition::Reaped);
}

/// No production field or arbitrary token input exists for this diagnostic.
#[tokio::test]
#[ignore = "requires verified network-none/read-only container; uses a fixed invalid non-secret local auth-presence token"]
async fn sandboxed_real_synthetic_auth_control_probe() {
    assert_eq!(
        std::env::var("MEMORY_PI_SANDBOXED_PROBE").unwrap(),
        "network-none-read-only-root"
    );
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
    config.synthetic_auth_presence = true;
    let report = probe(&config).await;
    println!("{}", serde_json::to_string(&report).unwrap());
    assert!(!report.provider_execution_enabled);
    assert_eq!(report.process, ProcessDisposition::Reaped);
    assert!(
        report.protocol_ready,
        "real control probe did not confirm the fixed state"
    );
}
