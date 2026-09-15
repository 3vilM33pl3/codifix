// SPDX-License-Identifier: AGPL-3.0-or-later
//! Trusted-host provider result persistence, separate from the metadata journal.
//! Does not authorize paid calls or authenticate provider receipts. Keep this
//! directory outside agent mounts and protect it from other writers.
use super::{
    budget::{Allowance, RequestBudget},
    digest,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_RECEIPT: usize = 2 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u8,
    campaign_sha256: String,
    task_id: String,
    request_id: String,
    payload_sha256: String,
    allowance: Allowance,
    usage: Allowance,
    output: Value,
}

pub struct ProviderResults {
    directory: PathBuf,
}

/// Trusted async dispatchers must durably reserve before invoking a provider.
/// A cached result does not authorize another invocation.
#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DispatchDecision {
    Cached { output: Value },
    Reserved,
}

impl ProviderResults {
    pub fn create(directory: &Path) -> Result<Self> {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(directory)?;
        #[cfg(unix)]
        File::open(
            directory
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?
        .sync_all()?;
        Self::open(directory)
    }

    pub fn open(directory: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(directory)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "result directory must not be a link"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                metadata.permissions().mode() & 0o077 == 0,
                "result directory must be private"
            );
        }
        Ok(Self {
            directory: directory.canonicalize()?,
        })
    }

    fn path(&self, request_id: &str) -> PathBuf {
        self.directory
            .join(format!("{}.json", digest(request_id.as_bytes())))
    }

    /// Exact completed retries return saved output without another invocation.
    /// Pending attempts require explicit recovery even if a receipt is present.
    pub fn dispatch(
        &self,
        budget: &mut RequestBudget,
        task_id: &str,
        request_id: &str,
        payload_sha256: &str,
        allowance: Allowance,
        provider: impl FnOnce() -> Result<(Value, Allowance)>,
    ) -> Result<Value> {
        match self.begin(budget, task_id, request_id, payload_sha256, allowance)? {
            DispatchDecision::Cached { output } => return Ok(output),
            DispatchDecision::Reserved => {}
        }
        let (output, usage) = provider()
            .map_err(|_| anyhow::anyhow!("provider outcome uncertain; recovery required"))?;
        self.complete_reserved(budget, task_id, request_id, payload_sha256, output, usage)
    }

    /// Persist the reservation or return an exact previously completed result.
    /// Cancellation after Reserved remains uncertain even if no bytes were sent.
    pub fn begin(
        &self,
        budget: &mut RequestBudget,
        task_id: &str,
        request_id: &str,
        payload_sha256: &str,
        allowance: Allowance,
    ) -> Result<DispatchDecision> {
        if let Some(saved) = budget.saved_request(request_id) {
            ensure!(
                saved.task_id == Some(task_id)
                    && saved.payload_sha256 == payload_sha256
                    && saved.allowance == allowance,
                "changed provider retry"
            );
            ensure!(
                saved.usage.is_some(),
                "provider attempt uncertain; explicit recovery required"
            );
            return Ok(DispatchDecision::Cached {
                output: self.recover(budget, request_id)?,
            });
        }
        ensure!(
            !self.path(request_id).try_exists()?,
            "orphan provider result requires investigation"
        );
        budget.reserve_for_task(task_id, request_id, payload_sha256, allowance)?;
        Ok(DispatchDecision::Reserved)
    }

    /// Async dispatchers reserve before awaiting the provider, then persist a
    /// validated complete response here. Cancellation before this remains uncertain.
    pub fn complete_reserved(
        &self,
        budget: &mut RequestBudget,
        task_id: &str,
        request_id: &str,
        payload_sha256: &str,
        output: Value,
        usage: Allowance,
    ) -> Result<Value> {
        let saved = budget
            .saved_request(request_id)
            .context("missing result reservation")?;
        ensure!(
            saved.usage.is_none()
                && saved.task_id == Some(task_id)
                && saved.payload_sha256 == payload_sha256,
            "result reservation mismatch"
        );
        let allowance = saved.allowance;
        let receipt = Receipt {
            version: 1,
            campaign_sha256: budget.identity().into(),
            task_id: task_id.into(),
            request_id: request_id.into(),
            payload_sha256: payload_sha256.into(),
            allowance,
            usage,
            output,
        };
        Self::validate_usage(&receipt)?;
        let bytes = serde_json::to_vec(&receipt)?;
        ensure!(
            bytes.len() <= MAX_RECEIPT,
            "provider result too large; usage remains reserved"
        );
        self.persist(request_id, &bytes)?;
        budget.settle_result(request_id, usage, Some(digest(&bytes)))?;
        Ok(receipt.output)
    }

    fn persist(&self, id: &str, bytes: &[u8]) -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(self.path(id))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        #[cfg(unix)]
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }

    fn validate_usage(receipt: &Receipt) -> Result<()> {
        ensure!(
            receipt.usage.input_tokens <= receipt.allowance.input_tokens
                && receipt.usage.output_tokens <= receipt.allowance.output_tokens
                && receipt.usage.microusd <= receipt.allowance.microusd,
            "invalid provider usage; reservation retained"
        );
        Ok(())
    }

    /// No provider is called here. A complete durable result from the interrupted
    /// dispatcher can settle its reservation; absence/corruption cannot refund it.
    pub fn recover_matching(
        &self,
        budget: &mut RequestBudget,
        task_id: &str,
        request_id: &str,
        payload_sha256: &str,
        allowance: Allowance,
    ) -> Result<Value> {
        let saved = budget
            .saved_request(request_id)
            .context("unknown provider request")?;
        ensure!(
            saved.task_id == Some(task_id)
                && saved.payload_sha256 == payload_sha256
                && saved.allowance == allowance,
            "recovery request binding mismatch"
        );
        self.recover(budget, request_id)
    }

    /// Recover a previously reserved identity without invoking its provider.
    pub fn recover(&self, budget: &mut RequestBudget, request_id: &str) -> Result<Value> {
        let saved = budget
            .saved_request(request_id)
            .context("unknown provider request")?;
        let path = self.path(request_id);
        let metadata = fs::symlink_metadata(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            ensure!(
                metadata.permissions().mode() & 0o077 == 0 && metadata.nlink() == 1,
                "provider result must be private and singly linked"
            );
        }
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() <= MAX_RECEIPT as u64,
            "invalid provider result file"
        );
        let mut bytes = vec![];
        File::open(&path)?
            .take(MAX_RECEIPT as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= MAX_RECEIPT, "oversized result");
        let receipt: Receipt = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("corrupt provider result"))?;
        ensure!(
            receipt.version == 1
                && receipt.campaign_sha256 == budget.identity()
                && receipt.request_id == request_id
                && Some(receipt.task_id.as_str()) == saved.task_id
                && receipt.payload_sha256 == saved.payload_sha256
                && receipt.allowance == saved.allowance,
            "provider receipt identity mismatch"
        );
        Self::validate_usage(&receipt)?;
        let hash = digest(&bytes);
        if let Some(usage) = saved.usage {
            ensure!(
                usage == receipt.usage && saved.result_sha256 == Some(hash.as_str()),
                "provider receipt digest mismatch"
            );
        } else {
            // Recovery may follow a process crash before the original fsync.
            File::open(&path)?.sync_all()?;
            #[cfg(unix)]
            File::open(&self.directory)?.sync_all()?;
            budget.settle_result(request_id, receipt.usage, Some(hash))?;
        }
        Ok(receipt.output)
    }
}

#[cfg(test)]
mod tests;
