// SPDX-License-Identifier: AGPL-3.0-or-later
//! R3 orchestrator: no ambient ApiClient/config/provider credentials are used.
use super::runtime::EvalCampaignCommand;
use anyhow::{Context, Result, ensure};
use mem_eval::campaign::budget::{Allowance, RequestBudget};
use mem_eval::campaign::provider_results::ProviderResults;
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
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec_pretty(data)?)?;
    file.sync_all()?;
    Ok(())
}

mod provider_mediation;
#[cfg(test)]
mod tests;

pub(super) async fn handle(command: EvalCampaignCommand) -> Result<()> {
    match command {
        EvalCampaignCommand::Review {
            manifest,
            artifact,
            out,
            ballots,
        } => {
            let manifest = Manifest::load(&manifest)?;
            let artifact: Artifact = bounded_json(&artifact)?;
            let packets = c::review::prepare(&manifest, &artifact)?;
            let updated = ballots
                .as_ref()
                .map(|path| {
                    let batch: c::review::ReviewBatch = bounded_json(path)?;
                    c::review::apply(&manifest, &artifact, &batch)
                })
                .transpose()?;
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder
                .create(&out)
                .context("review output must be a new directory")?;
            exclusive_json(
                &out.join("source-artifact-sha256.json"),
                &c::digest_value(&artifact)?,
            )?;
            if let Some(updated) = updated {
                exclusive_json(&out.join("artifact.json"), &updated)?;
                let report = c::report(&manifest, &updated)?;
                exclusive_json(&out.join("report.json"), &report)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                builder.create(out.join("packets"))?;
                builder.create(out.join("ballots"))?;
                for packet in &packets {
                    exclusive_json(
                        &out.join("packets")
                            .join(format!("{}.json", packet.review_id)),
                        packet,
                    )?;
                    exclusive_json(
                        &out.join("ballots")
                            .join(format!("{}.json", packet.review_id)),
                        &packet.blank_ballot("pending-reviewer".into())?,
                    )?;
                }
                exclusive_json(
                    &out.join("batch-template.json"),
                    &c::review::ReviewBatch {
                        artifact_sha256: c::digest_value(&artifact)?,
                        updates: vec![],
                    },
                )?;
                println!(
                    "Prepared {} blinded packets. Only packets/ and blank ballots/ belong with reviewers; no provider calls were made.",
                    packets.len()
                );
            }
        }
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
            resume,
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
            if !resume {
                ensure!(
                    !out.exists(),
                    "output already exists; use explicit --resume after inspecting interrupted evidence"
                );
                let mut builder = std::fs::DirBuilder::new();
                builder.recursive(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                builder.create(&out)?;
            }
            let out = out.canonicalize()?;
            let plan = manifest.plan(requested)?;
            let mut artifact = Artifact {
                manifest_sha256: plan.manifest_sha256.clone(),
                split: requested,
                offline,
                completed: false,
                preparation_microusd: None,
                approvals,
                observations: vec![],
            };
            let evidence_out = if resume {
                let original: Artifact = bounded_json(&out.join("started.json"))?;
                let original_plan: c::Plan = bounded_json(&out.join("plan.json"))?;
                ensure!(
                    c::digest_value(&original)? == c::digest_value(&artifact)?
                        && c::digest_value(&original_plan)? == c::digest_value(&plan)?,
                    "resume campaign/start identity mismatch"
                );
                let evidence_out = out.join(format!("resume-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir(&evidence_out)?;
                evidence_out
            } else {
                exclusive_json(&out.join("plan.json"), &plan)?;
                exclusive_json(&out.join("started.json"), &artifact)?;
                out.clone()
            };
            let mut ledger = Ledger::default();
            let execution = execute(
                &manifest_path,
                &manifest,
                &driver,
                &driver_hash,
                &out,
                &plan,
                offline,
                resume,
                &mut ledger,
                &mut artifact,
            )
            .await;
            artifact.completed = execution.is_ok();
            exclusive_json(&evidence_out.join("artifact.json"), &artifact)?;
            exclusive_json(&evidence_out.join("ledger.json"), &ledger)?;
            if let Err(error) = execution {
                anyhow::bail!(
                    "campaign interrupted; reservations may include uncertain provider charges; evidence at {}: {error:#}",
                    evidence_out.display()
                );
            }
            let report = c::report(&manifest, &artifact)?;
            exclusive_json(&evidence_out.join("report.json"), &report)?;
            eprintln!("Campaign evidence: {}", evidence_out.display());
            println!("{}", serde_json::to_string_pretty(&report)?);
            ensure!(
                report.harness_valid,
                "driver completed but R3 evidence is invalid"
            );
        }
    }
    Ok(())
}

fn bounded_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    use std::io::Read;
    let mut bytes = vec![];
    std::fs::File::open(path)?
        .take(256 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 256 * 1024 * 1024,
        "oversized campaign review input"
    );
    Ok(serde_json::from_slice(&bytes)?)
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
    resume: bool,
    ledger: &mut Ledger,
    artifact: &mut Artifact,
) -> Result<()> {
    // Reserve a whole task here; per-call mediation is nested inside this cap.
    // Never expose this campaign-level journal owner to agents.
    let open_budget = if resume {
        RequestBudget::resume_scoped
    } else {
        RequestBudget::create_scoped
    };
    let mut dispatch_budget = open_budget(
        &out.join("dispatch-budget.jsonl"),
        &plan.manifest_sha256,
        manifest.limits.clone(),
        c::Limits {
            requests: 1,
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
        },
    )?;
    let dispatch_results = if resume {
        ProviderResults::open(&out.join("dispatch-results"))?
    } else {
        ProviderResults::create(&out.join("dispatch-results"))?
    };
    // Recover the entire reserved prefix before dispatching anything new. A
    // pending request without a complete receipt remains ambiguous, even offline.
    // Keep the original journal locked for the entire resumed execution.
    *ledger = dispatch_budget.ledger().clone();
    let recovered = recover_dispatches(
        manifest_path,
        manifest,
        driver_hash,
        out,
        plan,
        offline,
        &mut dispatch_budget,
        &dispatch_results,
    )?;
    *ledger = dispatch_budget.ledger().clone();
    let mut provider_requests = 0u64;
    for (index, work) in plan.work.iter().enumerate() {
        // Aggregate per-task token ceiling. Driver must enforce this across all
        // its provider requests; reservations remain charged after uncertain exit.
        let tokens = manifest
            .model
            .max_input_tokens
            .checked_add(manifest.model.max_output_tokens)
            .context("token bound overflow")?;
        let priced = Allowance::for_model(
            &manifest.model,
            manifest.model.max_input_tokens,
            manifest.model.max_output_tokens,
        )?;
        let cost = if offline { 0 } else { priced.microusd };
        if let Some(observation) = recovered.get(index) {
            provider_requests = provider_requests
                .checked_add(observation.provider_requests)
                .context("provider request overflow")?;
            if index == 0 {
                artifact.preparation_microusd = Some(bounded_json(
                    &out.join("work-00000/preparation-microusd.json"),
                )?);
            }
            artifact.observations.push(observation.clone());
            continue;
        }
        // Confirmed receipts have already released unused reservations in the
        // durable owner. Do not charge the recovered prefix a second time or
        // retain settled worst-case tokens in this reporting/preflight ledger.
        *ledger = dispatch_budget.ledger().clone();
        ledger.reserve(&manifest.limits, tokens, cost)?;
        verify_campaign_inputs(manifest_path, &plan.manifest_sha256)?;
        let dir = out.join(format!("work-{index:05}"));
        std::fs::create_dir(&dir)?;
        std::fs::create_dir(dir.join("home"))?;
        exclusive_json(&dir.join("reservation.json"), ledger)?;
        let dispatch_id = format!("work-{index:05}");
        let provider_contract = provider_mediation::contract(
            manifest,
            &plan.manifest_sha256,
            &dir,
            &dispatch_id,
            offline,
            provider_requests,
        )?;
        provider_mediation::prepare(provider_contract.as_ref())?;
        let mut request = dispatch_request(manifest_path, plan, work, &dir, offline, tokens, cost);
        provider_mediation::bind(&mut request, provider_contract.as_ref())?;
        exclusive_json(&dir.join("request.json"), &request)?;
        let request_hash = c::digest_value(&request)?;
        dispatch_budget.reserve_for_task(
            &dispatch_id,
            &dispatch_id,
            &request_hash,
            Allowance {
                input_tokens: manifest.model.max_input_tokens,
                output_tokens: manifest.model.max_output_tokens,
                microusd: cost,
            },
        )?;
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
        provider_mediation::verify(
            manifest,
            &plan.manifest_sha256,
            &dispatch_id,
            offline,
            provider_contract.as_ref(),
            &observation,
        )?;
        provider_requests = provider_requests
            .checked_add(observation.provider_requests)
            .context("provider request overflow")?;
        // Preparation receipt is mandatory and separate from task execution.
        // Only the first task can prepare; later tasks must restore its snapshot.
        if index == 0 {
            let path = dir.join("preparation-microusd.json");
            artifact.preparation_microusd = Some(serde_json::from_slice(
                &std::fs::read(path).context("missing preparation receipt")?,
            )?);
        }
        dispatch_results.complete_reserved(
            &mut dispatch_budget,
            &dispatch_id,
            &dispatch_id,
            &request_hash,
            serde_json::to_value(&observation)?,
            Allowance {
                // Some providers report cache tokens outside input_tokens.
                // Account for every reported token, without adding caches twice.
                input_tokens: usage
                    .total_tokens
                    .checked_sub(usage.output_tokens)
                    .context("inconsistent total usage")?,
                output_tokens: usage.output_tokens,
                microusd: observation
                    .execution_microusd
                    .context("missing confirmed spend")?,
            },
        )?;
        *ledger = dispatch_budget.ledger().clone();
        artifact.observations.push(observation);
    }
    verify_campaign_inputs(manifest_path, &plan.manifest_sha256)?;
    Ok(())
}

fn dispatch_request(
    manifest_path: &Path,
    plan: &c::Plan,
    work: &c::Work,
    dir: &Path,
    offline: bool,
    tokens: u64,
    cost: u64,
) -> serde_json::Value {
    serde_json::json!({"version":1,"manifest":manifest_path,"manifest_sha256":plan.manifest_sha256,
        "work":work,"offline":offline,"maximum_tokens":tokens,"maximum_microusd":cost,
        "response":dir.join("observation.json")})
}

#[allow(clippy::too_many_arguments)]
fn recover_dispatches(
    manifest_path: &Path,
    manifest: &Manifest,
    driver_hash: &str,
    out: &Path,
    plan: &c::Plan,
    offline: bool,
    budget: &mut RequestBudget,
    results: &ProviderResults,
) -> Result<Vec<Observation>> {
    verify_campaign_inputs(manifest_path, &plan.manifest_sha256)?;
    let ids: Vec<String> = budget
        .request_ids()
        .into_iter()
        .map(str::to_owned)
        .collect();
    ensure!(ids.len() <= plan.work.len(), "unexpected saved dispatches");
    let tokens = manifest
        .model
        .max_input_tokens
        .checked_add(manifest.model.max_output_tokens)
        .context("token bound overflow")?;
    let mut allowance = Allowance::for_model(
        &manifest.model,
        manifest.model.max_input_tokens,
        manifest.model.max_output_tokens,
    )?;
    if offline {
        allowance.microusd = 0;
    }
    let mut recovered = vec![];
    let mut provider_requests = 0u64;
    for (index, id) in ids.iter().enumerate() {
        ensure!(
            *id == format!("work-{index:05}"),
            "saved dispatch prefix has gaps"
        );
        let work = &plan.work[index];
        let dir = out.join(id);
        let mut expected = dispatch_request(
            manifest_path,
            plan,
            work,
            &dir,
            offline,
            tokens,
            allowance.microusd,
        );
        let provider_contract = provider_mediation::contract(
            manifest,
            &plan.manifest_sha256,
            &dir,
            id,
            offline,
            provider_requests,
        )?;
        provider_mediation::bind(&mut expected, provider_contract.as_ref())?;
        let saved: serde_json::Value = bounded_json(&dir.join("request.json"))?;
        ensure!(saved == expected, "saved dispatch request changed");
        let observation: Observation = serde_json::from_value(
            results
                .recover_matching(budget, id, id, &c::digest_value(&expected)?, allowance)
                .context("saved dispatch is ambiguous or invalid; no new work started")?,
        )?;
        validate_task_evidence(manifest, work, &observation, recovered.first())?;
        ensure!(
            observation.isolation.driver_sha256 == driver_hash,
            "saved driver mismatch"
        );
        provider_mediation::verify(
            manifest,
            &plan.manifest_sha256,
            id,
            offline,
            provider_contract.as_ref(),
            &observation,
        )?;
        provider_requests = provider_requests
            .checked_add(observation.provider_requests)
            .context("provider request overflow")?;
        let usage = observation
            .result
            .token_usage
            .as_ref()
            .context("saved usage missing")?;
        ensure!(
            usage.total_tokens <= tokens
                && observation
                    .execution_microusd
                    .is_some_and(|cost| cost <= allowance.microusd),
            "saved observation exceeds allowance"
        );
        recovered.push(observation);
    }
    if !recovered.is_empty() {
        let _: u64 = bounded_json(&out.join("work-00000/preparation-microusd.json"))?;
    }
    Ok(recovered)
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
    // Supplied semantic evidence must agree with the locked sources/labels even
    // in offline mode; absence remains allowed only for diagnostic smoke runs.
    c::scoring::validate_pending_observation(manifest, observation)?;
    ensure!(
        observation.model == manifest.model,
        "driver model/configuration mismatch"
    );
    ensure!(
        observation.result.item_id == work.case_id
            && !observation.result.skipped
            && observation.result.duration_ms.is_some(),
        "missing or unresolved task evidence"
    );
    ensure!(
        !observation.uncertain_charge,
        "uncertain provider charge; stop campaign"
    );
    if let Some(timing) = &observation.timing {
        timing.validate(observation.result.duration_ms)?;
    } else {
        ensure!(
            manifest.provider_budget.is_none(),
            "mediated execution requires execution-only timing"
        );
    }
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
            && usage
                .total_tokens
                .checked_sub(usage.output_tokens)
                .is_some_and(|input| input <= manifest.model.max_input_tokens)
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
