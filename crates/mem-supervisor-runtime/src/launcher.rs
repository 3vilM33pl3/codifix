//! Trusted launcher contracts; platform implementations enforce the sandbox.
use crate::{
    model::PreparedRequest,
    pi_broker::{BrokerFailure, BrokerJob, BrokerReply, JobFuture},
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::watch;
#[cfg(unix)]
#[path = "launcher/linux.rs"]
mod platform;
#[cfg(windows)]
#[path = "launcher/windows.rs"]
mod platform;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxConfig {
    pub pi: PathBuf,
    pub driver: PathBuf,
    pub driver_sha256: String,
    pub private_runtime: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows_artifacts: Option<mem_supervisor_pi::WindowsArtifact>,
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
    #[cfg(unix)]
    let task = tokio::spawn(platform::run(config, request, job, control));
    // The Win32 ACL/profile allocations remain confined to one owning thread;
    // do not claim Send for native security descriptor interior pointers.
    #[cfg(windows)]
    let task = tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(platform::run(config, request, job, control))
    });
    task.await.map_err(|_| LaunchError::Cleanup)?
}
