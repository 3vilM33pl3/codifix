// SPDX-License-Identifier: AGPL-3.0-or-later
//! A distinct bounded driver phase, completed before task dispatch. Never rerun
//! interrupted preparation automatically, even when no charge is yet known.
use super::*;
use c::preparation::PreparationReceipt;

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    manifest_path: &Path,
    manifest: &Manifest,
    driver: &Path,
    driver_hash: &str,
    out: &Path,
    plan: &c::Plan,
    offline: bool,
    resume: bool,
) -> Result<PreparationReceipt> {
    let limits = manifest
        .preparation_budget
        .as_ref()
        .context("missing preparation allocation")?;
    c::preparation::validate_limits(&manifest.limits, limits)?;
    ensure!(
        !offline || limits.microusd == 0,
        "offline preparation must have zero spend allowance"
    );
    verify_campaign_inputs(manifest_path, &plan.manifest_sha256)?;
    let dir = out.join("preparation");
    let request = serde_json::json!({
        "version": 1, "phase": "preparation", "manifest": manifest_path,
        "manifest_sha256": plan.manifest_sha256, "offline": offline,
        "maximum_microusd": limits.microusd, "response": dir.join("receipt.json"),
        "provider_budget": { "directory": dir.join("provider"), "manifest_sha256": plan.manifest_sha256,
            "task_id": "preparation", "limits": limits, "task_limits": limits },
    });
    if resume {
        let saved: serde_json::Value = bounded_json(&dir.join("request.json"))?;
        ensure!(saved == request, "changed preparation request");
        let accepted: PreparationReceipt = bounded_json(&dir.join("accepted.json"))
            .context("preparation interrupted; inspect retained receipts, no automatic restart")?;
        let current = verify(&dir, manifest, &plan.manifest_sha256)?;
        ensure!(
            c::digest_value(&accepted)? == c::digest_value(&current)?,
            "changed preparation evidence"
        );
        return Ok(current);
    }
    let builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    let mut builder = builder;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&dir)?;
    builder.create(dir.join("home"))?;
    builder.create(dir.join("provider"))?;
    exclusive_json(&dir.join("request.json"), &request)?;
    ensure!(
        c::digest(&std::fs::read(driver)?) == driver_hash,
        "preparation driver changed"
    );
    let log = |name| {
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(dir.join(name))
    };
    let mut command = tokio::process::Command::new(driver);
    command
        .arg(dir.join("request.json"))
        .current_dir(&dir)
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("HOME", dir.join("home"))
        .env("MEMORY_LAYER_PROFILE", "prod")
        .env("MEMORY_R3_OFFLINE", if offline { "1" } else { "0" })
        .stdin(Stdio::null())
        .stdout(log("driver.stdout")?)
        .stderr(log("driver.stderr")?)
        .kill_on_drop(true);
    let status = tokio::time::timeout(
        std::time::Duration::from_secs(limits.task_seconds),
        command.status(),
    )
    .await
    .context("preparation deadline; retain uncertain evidence")??;
    ensure!(
        status.success(),
        "preparation failed; retain uncertain evidence"
    );
    verify_campaign_inputs(manifest_path, &plan.manifest_sha256)?;
    let receipt = verify(&dir, manifest, &plan.manifest_sha256)?;
    exclusive_json(&dir.join("accepted.json"), &receipt)?;
    Ok(receipt)
}

fn verify(dir: &Path, manifest: &Manifest, manifest_sha256: &str) -> Result<PreparationReceipt> {
    let limits = manifest
        .preparation_budget
        .as_ref()
        .context("missing preparation limits")?;
    let receipt: PreparationReceipt = bounded_json(&dir.join("receipt.json"))?;
    receipt.execution_limits(&manifest.limits, limits, manifest_sha256)?;
    let provider = dir.join("provider");
    let mut budget = RequestBudget::resume_scoped(
        &provider.join("budget.jsonl"),
        manifest_sha256,
        limits.clone(),
        limits.clone(),
    )?;
    let results = ProviderResults::open(&provider.join("results"))?;
    let actual = results.audit(&mut budget)?;
    ensure!(
        c::digest_value(&actual)? == c::digest_value(&receipt.audit)?,
        "preparation audit differs from actual journal/receipts"
    );
    Ok(receipt)
}
