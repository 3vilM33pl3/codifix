// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bind each driver task to its actual per-call journal, not a claimed count.
use super::*;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub(super) struct Contract {
    directory: PathBuf,
    manifest_sha256: String,
    task_id: String,
    limits: c::Limits,
    task_limits: c::Limits,
}

pub(super) fn contract(
    manifest: &Manifest,
    manifest_sha256: &str,
    dir: &Path,
    task_id: &str,
    offline: bool,
    used_requests: u64,
) -> Result<Option<Contract>> {
    let Some(profile) = &manifest.provider_budget else {
        return Ok(None);
    };
    let remaining = manifest
        .limits
        .requests
        .checked_sub(used_requests)
        .context("provider request overflow")?;
    ensure!(remaining > 0, "campaign provider request limit exhausted");
    let limits = c::Limits {
        requests: profile.max_calls_per_task.min(remaining),
        tokens: manifest
            .model
            .max_input_tokens
            .checked_add(manifest.model.max_output_tokens)
            .context("task token overflow")?,
        microusd: if offline {
            0
        } else {
            Allowance::for_model(
                &manifest.model,
                manifest.model.max_input_tokens,
                manifest.model.max_output_tokens,
            )?
            .microusd
        },
        task_seconds: manifest.limits.task_seconds,
    };
    Ok(Some(Contract {
        directory: dir.join("provider"),
        manifest_sha256: manifest_sha256.into(),
        task_id: task_id.into(),
        task_limits: limits.clone(),
        limits,
    }))
}

pub(super) fn prepare(contract: Option<&Contract>) -> Result<()> {
    if let Some(contract) = contract {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&contract.directory)?;
    }
    Ok(())
}

pub(super) fn bind(request: &mut serde_json::Value, contract: Option<&Contract>) -> Result<()> {
    if let Some(contract) = contract {
        request["provider_budget"] = serde_json::to_value(contract)?;
    }
    Ok(())
}

pub(super) fn verify(
    manifest: &Manifest,
    manifest_sha256: &str,
    task_id: &str,
    offline: bool,
    contract: Option<&Contract>,
    observation: &Observation,
) -> Result<()> {
    observation.validate_provider_audit(manifest, manifest_sha256, task_id, offline)?;
    let Some(contract) = contract else {
        ensure!(
            observation.provider_requests <= 1,
            "multi-request driver requires a locked per-call mediation profile"
        );
        return Ok(());
    };
    // This acquires the actual journal lock. A still-running owner, unfinished
    // reservation, altered receipt or differing request cap fails before another
    // driver starts. No provider is invoked by verification or recovery.
    let mut budget = RequestBudget::resume_scoped(
        &contract.directory.join("budget.jsonl"),
        &contract.manifest_sha256,
        contract.limits.clone(),
        contract.task_limits.clone(),
    )?;
    let results = ProviderResults::open(&contract.directory.join("results"))?;
    let actual = results.audit(&mut budget)?;
    ensure!(
        c::digest_value(&actual)?
            == c::digest_value(
                observation
                    .provider_audit
                    .as_ref()
                    .context("missing provider audit")?
            )?,
        "provider evidence differs from actual journal/receipts"
    );
    Ok(())
}
