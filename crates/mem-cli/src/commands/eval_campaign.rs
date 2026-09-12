// SPDX-License-Identifier: AGPL-3.0-or-later
//! R3 orchestrator: no ambient ApiClient/config/provider credentials are used.
use super::runtime::EvalCampaignCommand;
use anyhow::{Context, Result, ensure};
use mem_eval::campaign::{self as c, Artifact, Ledger, Manifest, Observation, Split};
use std::{fs::OpenOptions, io::Write, path::Path, process::Stdio};

fn split(value: &str) -> Result<Split> {
    match value {
        "development" => Ok(Split::Development),
        "held-out" => Ok(Split::HeldOut),
        _ => anyhow::bail!("split must be development or held-out"),
    }
}

fn exclusive_json<T: serde::Serialize>(path: &Path, data: &T) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&serde_json::to_vec_pretty(data)?)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests;

pub(super) async fn handle(command: EvalCampaignCommand) -> Result<()> {
    match command {
        EvalCampaignCommand::Plan {
            manifest,
            split: requested,
        } => {
            let manifest = Manifest::load(&manifest)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&manifest.plan(split(&requested)?)?)?
            );
        }
        EvalCampaignCommand::Report { manifest, artifact } => {
            let manifest = Manifest::load(&manifest)?;
            let artifact: Artifact = serde_json::from_slice(&std::fs::read(artifact)?)?;
            let report = c::report(&manifest, &artifact)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            ensure!(report.harness_valid, "invalid R3 evidence");
            if report.official_evidence {
                ensure!(
                    report.quality_passed,
                    "valid campaign did not pass product quality"
                );
            }
        }
        EvalCampaignCommand::Run {
            manifest,
            driver,
            out,
            split: requested,
            allow_shell,
            offline,
            approvals,
        } => {
            // Fail before dispatch until the multi-request provider budget proxy
            // and live isolation driver are independently verified. Approval
            // files alone must not unlock incomplete paid-execution machinery.
            ensure!(
                offline,
                "paid campaign execution remains unavailable pending provider-budget mediation and live isolation verification"
            );
            ensure!(
                allow_shell,
                "campaign run requires --allow-shell after reviewing the isolation driver"
            );
            let manifest_path = manifest.canonicalize()?;
            let manifest = Manifest::load(&manifest_path)?;
            let requested = split(&requested)?;
            let approvals = approvals
                .iter()
                .map(|p| Ok(serde_json::from_slice(&std::fs::read(p)?)?))
                .collect::<Result<Vec<c::Approval>>>()?;
            if !offline {
                manifest.authorize(requested, &approvals)?;
            }
            let driver = driver.canonicalize()?;
            let driver_hash = c::digest(&std::fs::read(&driver)?);
            let root = manifest_path.parent().context("campaign directory")?;
            ensure!(
                manifest
                    .assets
                    .iter()
                    .any(|a| a.kind == c::AssetKind::Configuration
                        && a.sha256 == driver_hash
                        && root.join(&a.path).canonicalize().ok().as_ref() == Some(&driver)),
                "driver is not a locked campaign configuration asset"
            );
            ensure!(
                !out.exists(),
                "output already exists; inspect interrupted evidence, never overwrite or blindly replay"
            );
            std::fs::create_dir_all(&out)?;
            let out = out.canonicalize()?;
            let plan = manifest.plan(requested)?;
            exclusive_json(&out.join("plan.json"), &plan)?;
            let mut artifact = Artifact {
                manifest_sha256: plan.manifest_sha256.clone(),
                split: requested,
                offline,
                completed: false,
                preparation_microusd: None,
                approvals,
                observations: vec![],
            };
            exclusive_json(&out.join("started.json"), &artifact)?;
            let mut ledger = Ledger::default();
            let execution = execute(
                &manifest_path,
                &manifest,
                &driver,
                &driver_hash,
                &out,
                &plan,
                offline,
                &mut ledger,
                &mut artifact,
            )
            .await;
            artifact.completed = execution.is_ok();
            exclusive_json(&out.join("artifact.json"), &artifact)?;
            exclusive_json(&out.join("ledger.json"), &ledger)?;
            if let Err(error) = execution {
                anyhow::bail!(
                    "campaign interrupted; reservations may include uncertain provider charges; evidence at {}: {error:#}",
                    out.display()
                );
            }
            let report = c::report(&manifest, &artifact)?;
            exclusive_json(&out.join("report.json"), &report)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            ensure!(
                report.harness_valid,
                "driver completed but R3 evidence is invalid"
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn execute(
    manifest_path: &Path,
    manifest: &Manifest,
    driver: &Path,
    driver_hash: &str,
    out: &Path,
    plan: &c::Plan,
    offline: bool,
    ledger: &mut Ledger,
    artifact: &mut Artifact,
) -> Result<()> {
    for (index, work) in plan.work.iter().enumerate() {
        verify_campaign_inputs(manifest_path, &plan.manifest_sha256)?;
        // Aggregate per-task token ceiling. Driver must enforce this across all
        // its provider requests; reservations remain charged after uncertain exit.
        let tokens = manifest
            .model
            .max_input_tokens
            .checked_add(manifest.model.max_output_tokens)
            .context("token bound overflow")?;
        let estimated = (u128::from(manifest.model.max_input_tokens)
            * u128::from(manifest.model.input_microusd_per_million)
            + u128::from(manifest.model.max_output_tokens)
                * u128::from(manifest.model.output_microusd_per_million))
        .div_ceil(1_000_000);
        let cost = if offline {
            0
        } else {
            u64::try_from(estimated).context("cost bound overflow")?
        };
        ledger.reserve(&manifest.limits, tokens, cost)?;
        let dir = out.join(format!("work-{index:05}"));
        std::fs::create_dir(&dir)?;
        std::fs::create_dir(dir.join("home"))?;
        exclusive_json(&dir.join("reservation.json"), ledger)?;
        let request = serde_json::json!({"version":1,"manifest":manifest_path,"manifest_sha256":plan.manifest_sha256,
            "work":work,"offline":offline,"maximum_tokens":tokens,"maximum_microusd":cost,
            "response":dir.join("observation.json")});
        exclusive_json(&dir.join("request.json"), &request)?;
        let stdout = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(dir.join("driver.stdout"))?;
        let stderr = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(dir.join("driver.stderr"))?;
        ensure!(
            c::digest(&std::fs::read(driver)?) == driver_hash,
            "driver changed during campaign"
        );
        let mut process = tokio::process::Command::new(driver);
        process
            .arg(dir.join("request.json"))
            .current_dir(&dir)
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("HOME", dir.join("home"))
            .env("MEMORY_LAYER_PROFILE", "prod")
            .env("MEMORY_R3_OFFLINE", if offline { "1" } else { "0" })
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .kill_on_drop(true);
        let status = tokio::time::timeout(
            std::time::Duration::from_secs(manifest.limits.task_seconds),
            process.status(),
        )
        .await
        .context("driver deadline expired; owned containers require reconciliation")??;
        ensure!(
            status.success(),
            "driver failed; inspect sanitized driver logs"
        );
        // Check the last work item too, before accepting evidence or dispatching
        // anything else. A driver cannot rewrite the manifest to bless new assets.
        verify_campaign_inputs(manifest_path, &plan.manifest_sha256)?;
        let observation_path = dir.join("observation.json");
        ensure!(
            std::fs::metadata(&observation_path)?.len() <= 16 * 1024 * 1024,
            "oversized observation"
        );
        let observation: Observation = serde_json::from_slice(&std::fs::read(&observation_path)?)?;
        validate_task_evidence(manifest, work, &observation, artifact.observations.first())?;
        ensure!(observation.work == *work, "driver returned another task");
        observation.isolation.validate(manifest)?;
        ensure!(
            observation.isolation.driver_sha256 == driver_hash,
            "driver evidence identity mismatch"
        );
        let usage = observation
            .result
            .token_usage
            .as_ref()
            .context("unknown task token usage")?;
        ensure!(
            usage.total_tokens <= tokens,
            "driver exceeded token reservation"
        );
        ensure!(
            observation.execution_microusd.is_some_and(|c| c <= cost),
            "unknown/excess task spend"
        );
        // More than one call consumes additional request quota. The driver must
        // enforce the supplied remaining request limit before each dispatch.
        ensure!(
            observation.provider_requests <= 1,
            "multi-request driver needs per-request budget mediation; unsupported"
        );
        artifact.observations.push(observation);
        // Preparation receipt is mandatory and separate from task execution.
        // Only the first task can prepare; later tasks must restore its snapshot.
        if index == 0 {
            let path = dir.join("preparation-microusd.json");
            artifact.preparation_microusd = Some(serde_json::from_slice(
                &std::fs::read(path).context("missing preparation receipt")?,
            )?);
        }
    }
    Ok(())
}

fn verify_campaign_inputs(path: &Path, expected_hash: &str) -> Result<()> {
    let current = Manifest::load(path)?;
    ensure!(
        c::digest_value(&current)? == expected_hash,
        "campaign manifest changed during execution"
    );
    Ok(())
}

/// Fail before another task starts, not only when the final report is built.
/// Incorrect answers are valid observations; broken measurement is not.
fn validate_task_evidence(
    manifest: &Manifest,
    work: &c::Work,
    observation: &Observation,
    first: Option<&Observation>,
) -> Result<()> {
    ensure!(observation.work == *work, "driver returned another task");
    ensure!(
        observation.model == manifest.model,
        "driver model/configuration mismatch"
    );
    ensure!(
        observation.result.item_id == work.case_id
            && !observation.result.skipped
            && observation.result.duration_ms.is_some()
            && !observation.unresolved_rubric,
        "missing or unresolved task evidence"
    );
    ensure!(
        !observation.uncertain_charge,
        "uncertain provider charge; stop campaign"
    );
    let usage = observation
        .result
        .token_usage
        .as_ref()
        .context("unknown task token usage")?;
    ensure!(
        usage
            .input_tokens
            .checked_add(usage.output_tokens)
            .is_some_and(|sum| sum <= usage.total_tokens)
            && usage.input_tokens <= manifest.model.max_input_tokens
            && usage.output_tokens <= manifest.model.max_output_tokens,
        "inconsistent or excessive task token usage"
    );
    observation.isolation.validate(manifest)?;
    if let Some(first) = first {
        ensure!(
            observation.isolation.configuration_sha256 == first.isolation.configuration_sha256
                && observation.isolation.driver_sha256 == first.isolation.driver_sha256
                && observation.isolation.service_build == first.isolation.service_build,
            "mixed configuration/driver/build"
        );
    }
    Ok(())
}
