//! Fixed local Docker launcher. No host network, provider key, DB credentials,
//! Docker socket or workspace reaches Pi. Operator supplies hash-bound artifacts;
//! a model cannot choose executables, flags, mounts, images, endpoints or IDs.
use crate::{
    model::PreparedRequest,
    pi_admission::PiAdmission,
    pi_broker::{BrokerFailure, BrokerJob, BrokerReply, JobFuture, PiBroker},
    private_files,
};
use mem_supervisor_pi::{
    ProcessDisposition,
    sandbox::{ChildInput, ChildReport, PATCHED_AMD64_SHA256},
};
use serde::{Deserialize, Serialize};
use std::{os::unix::fs::PermissionsExt, path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::watch,
    time::timeout,
};
use uuid::Uuid;

const IMAGE: &str =
    "ubuntu@sha256:cd21a4f68a617580279d4b091cb18e3af9fa8a87500665f0ae5f7f757d17d367";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxConfig {
    pub pi: PathBuf,
    pub driver: PathBuf,
    pub driver_sha256: String,
    pub private_runtime: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LaunchError {
    #[error("private runtime files are unavailable or unsafe")]
    Files,
    #[error("artifact does not match its approved hash")]
    Artifact,
    #[error("supported local sandbox is unavailable")]
    Unavailable,
    #[error("work was stopped; inspect durable state before any new request")]
    Stopped,
    #[error("sandbox deadline elapsed; inspect durable accounting")]
    Deadline,
    #[error("child did not confirm bounded proposal delivery")]
    Child,
    #[error("sandbox cleanup is unconfirmed; operator intervention required")]
    Cleanup,
}

/// Irreversible cancellation shared by service lifecycle, ingress and guards.
#[derive(Clone)]
pub struct RunControl(watch::Sender<bool>);
impl Default for RunControl {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}
impl RunControl {
    pub fn stop(&self) {
        self.0.send_replace(true);
    }
    pub fn stopped(&self) -> bool {
        *self.0.borrow()
    }
    pub async fn cancelled(&self) {
        let mut rx = self.0.subscribe();
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                break;
            }
        }
    }
}

struct ControlledJob {
    inner: Arc<dyn BrokerJob>,
    control: RunControl,
}
impl BrokerJob for ControlledJob {
    fn request_digest(&self) -> &str {
        self.inner.request_digest()
    }
    fn run(&self) -> JobFuture<'_, Result<BrokerReply, BrokerFailure>> {
        Box::pin(async {
            if self.control.stopped() {
                return Err(BrokerFailure::Denied);
            }
            self.inner.run().await
        })
    }
    fn authorize_delivery(&self) -> JobFuture<'_, Result<(), BrokerFailure>> {
        Box::pin(async {
            if self.control.stopped() {
                return Err(BrokerFailure::Denied);
            }
            self.inner.authorize_delivery().await?;
            if self.control.stopped() {
                return Err(BrokerFailure::Denied);
            }
            Ok(())
        })
    }
}

/// Result digest is untrusted until compared with the ledger's exact proposal.
/// Cancelling this caller's wait does not cancel the bounded cleanup owner.
pub async fn launch(
    config: SandboxConfig,
    request: Arc<PreparedRequest>,
    job: Arc<dyn BrokerJob>,
    control: RunControl,
) -> Result<[u8; 32], LaunchError> {
    tokio::spawn(run(config, request, job, control))
        .await
        .map_err(|_| LaunchError::Cleanup)?
}

async fn run(
    config: SandboxConfig,
    request: Arc<PreparedRequest>,
    job: Arc<dyn BrokerJob>,
    control: RunControl,
) -> Result<[u8; 32], LaunchError> {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64"))
        || rustix::process::geteuid().as_raw() == 0
    {
        return Err(LaunchError::Unavailable);
    }
    if control.stopped() {
        return Err(LaunchError::Stopped);
    }
    let prepared = tokio::task::spawn_blocking(move || {
        private_files::directory(&config.private_runtime, true)?;
        private_files::digest(&config.driver_sha256)?;
        let dir = tempfile::Builder::new()
            .prefix("pi-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir_in(&config.private_runtime)
            .map_err(|_| LaunchError::Files)?;
        private_files::copy(&config.pi, &dir.path().join("pi"), PATCHED_AMD64_SHA256)?;
        private_files::copy(
            &config.driver,
            &dir.path().join("driver"),
            &config.driver_sha256,
        )?;
        let socket_dir = dir.path().join("broker");
        std::fs::create_dir(&socket_dir).map_err(|_| LaunchError::Files)?;
        std::fs::set_permissions(&socket_dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| LaunchError::Files)?;
        Ok::<_, LaunchError>(dir)
    })
    .await
    .map_err(|_| LaunchError::Files)??;
    if control.stopped() {
        return Err(LaunchError::Stopped);
    }
    let socket = prepared.path().join("broker/job.sock");
    let listener = tokio::net::UnixListener::bind(&socket).map_err(|_| LaunchError::Unavailable)?;
    tokio::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
        .await
        .map_err(|_| LaunchError::Files)?;
    let job = Arc::new(ControlledJob {
        inner: job,
        control: control.clone(),
    });
    let (broker, token) = PiBroker::new(Arc::clone(&request), job, Duration::from_secs(120))
        .map_err(|_| LaunchError::Unavailable)?;
    let admission = PiAdmission::new(&request);
    let input = serde_json::to_vec(&ChildInput {
        system: admission.system_prompt().into(),
        prompt: admission.prompt().into(),
        broker_token: token.bearer_for_child().into(),
    })
    .map_err(|_| LaunchError::Files)?;
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, broker.router()).await;
    });
    let uid = rustix::process::geteuid().as_raw();
    let name = format!("memory-pi-work-{}", Uuid::new_v4().simple());
    let args = arguments(&name, uid, prepared.path());
    let mut command = docker();
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(_) => {
            server.abort();
            return Err(LaunchError::Unavailable);
        }
    };
    let operation = async {
        let mut stdin = child.stdin.take().ok_or(LaunchError::Child)?;
        let mut stdout = child.stdout.take().ok_or(LaunchError::Child)?.take(8193);
        let mut stderr = child.stderr.take().ok_or(LaunchError::Child)?.take(8193);
        stdin
            .write_all(&input)
            .await
            .map_err(|_| LaunchError::Child)?;
        drop(stdin);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (a, b, status) = tokio::join!(
            stdout.read_to_end(&mut out),
            stderr.read_to_end(&mut err),
            child.wait()
        );
        a.map_err(|_| LaunchError::Child)?;
        b.map_err(|_| LaunchError::Child)?;
        if out.len() > 8192
            || err.len() > 8192
            || !status.map_err(|_| LaunchError::Child)?.success()
        {
            return Err(LaunchError::Child);
        }
        let report: ChildReport = serde_json::from_slice(&out).map_err(|_| LaunchError::Child)?;
        if report.process != ProcessDisposition::Reaped || report.failure.is_some() {
            return Err(LaunchError::Child);
        }
        report.proposal_sha256.ok_or(LaunchError::Child)
    };
    let result = tokio::select! {
        result=timeout(Duration::from_secs(125),operation)=>result.unwrap_or(Err(LaunchError::Deadline)),
        _=control.cancelled()=>Err(LaunchError::Stopped),
    };
    // Always close ingress and request owned-container termination. A detached
    // broker job may still settle a dispatched call; its disclosure guard must
    // also use this control before generation/publication. No automatic retry.
    server.abort();
    let _ = server.await;
    if result.is_err() {
        let _ = child.start_kill();
    }
    if !matches!(
        timeout(Duration::from_secs(2), child.wait()).await,
        Ok(Ok(_))
    ) {
        let _ = prepared.keep();
        return Err(LaunchError::Cleanup);
    }
    let cleanup = timeout(
        Duration::from_secs(5),
        docker()
            .args(["rm", "--force", &name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
    .await;
    // rm may report absent after --rm; authoritative bounded inspect decides.
    if cleanup.is_err() {
        let _ = prepared.keep();
        return Err(LaunchError::Cleanup);
    }
    let inspect = timeout(
        Duration::from_secs(5),
        docker()
            .args([
                "container",
                "ls",
                "--all",
                "--filter",
                &format!("name=^{name}$"),
                "--format",
                "{{.Names}}",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output(),
    )
    .await;
    if !matches!(inspect,Ok(Ok(ref output)) if output.status.success() && output.stdout.is_empty())
    {
        let _ = prepared.keep();
        return Err(LaunchError::Cleanup);
    }
    result
}

fn docker() -> tokio::process::Command {
    let mut command = tokio::process::Command::new("/usr/bin/docker");
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args(["--host", "unix:///var/run/docker.sock"])
        .kill_on_drop(true);
    command
}
fn arguments(name: &str, uid: u32, dir: &std::path::Path) -> Vec<String> {
    let mut args = [
        "run",
        "--rm",
        "--pull=never",
        "--network=none",
        "--read-only",
        "--cap-drop=ALL",
        "--security-opt=no-new-privileges",
        "--pids-limit=32",
        "--memory=512m",
        "--cpus=1",
        "--interactive",
        "--name",
        name,
    ]
    .map(str::to_owned)
    .to_vec();
    args.push(format!("--user={uid}:{uid}"));
    args.push(format!(
        "--tmpfs=/scratch:rw,exec,nosuid,nodev,size=160m,mode=0700,uid={uid},gid={uid}"
    ));
    for (source, target) in [
        ("pi", "/pi/pi"),
        ("driver", "/driver"),
        ("broker", "/broker"),
    ] {
        args.push(format!(
            "--mount=type=bind,src={},dst={target},readonly",
            dir.join(source).display()
        ));
    }
    args.extend([IMAGE.into(), "/driver".into()]);
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sandbox_flags_and_mounts_are_fixed_and_credentials_are_not_inherited() {
        let args = arguments(
            "memory-pi-work-test",
            1000,
            std::path::Path::new("/private/run"),
        );
        for flag in [
            "--network=none",
            "--read-only",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--pull=never",
        ] {
            assert!(args.iter().any(|arg| arg == flag));
        }
        assert_eq!(
            args.iter()
                .filter(|arg| arg.starts_with("--mount="))
                .count(),
            3
        );
        assert!(!args.iter().any(|arg| arg.contains("docker.sock")
            || arg.contains("OPENAI")
            || arg == "--privileged"));
        assert_eq!(args.last().map(String::as_str), Some("/driver"));
    }
    #[tokio::test]
    async fn cancellation_is_irreversible_and_wakes_existing_and_new_waiters() {
        let control = RunControl::default();
        let other = control.clone();
        let waiting = tokio::spawn(async move {
            other.cancelled().await;
        });
        control.stop();
        control.stop();
        timeout(Duration::from_secs(1), waiting)
            .await
            .expect("woken")
            .expect("join");
        timeout(Duration::from_secs(1), control.cancelled())
            .await
            .expect("already stopped");
        assert!(control.stopped());
    }
}
