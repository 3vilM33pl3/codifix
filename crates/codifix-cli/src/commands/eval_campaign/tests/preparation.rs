// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

#[test]
fn driver_child() {
    let Some(path) = std::env::var_os("CODIFIX_R3_PREPARATION_TEST_REQUEST") else {
        return;
    };
    let request: serde_json::Value = bounded_json(Path::new(&path)).unwrap();
    let contract = &request["provider_budget"];
    let limits: c::Limits = serde_json::from_value(contract["limits"].clone()).unwrap();
    let task_limits = serde_json::from_value(contract["task_limits"].clone()).unwrap();
    let directory = std::path::PathBuf::from(contract["directory"].as_str().unwrap());
    let mut budget = RequestBudget::create_scoped(
        &directory.join("budget.jsonl"),
        contract["manifest_sha256"].as_str().unwrap(),
        limits,
        task_limits,
    )
    .unwrap();
    let results = ProviderResults::create(&directory.join("results")).unwrap();
    let preparing = request["phase"] == "preparation";
    if preparing && Path::new("../../interrupt-preparation").exists() {
        budget
            .reserve_for_task(
                "preparation",
                "interrupted",
                &c::digest(b"fake-input"),
                Allowance {
                    input_tokens: 10,
                    output_tokens: 0,
                    microusd: 0,
                },
            )
            .unwrap();
        std::process::exit(17);
    }
    let usage = Allowance {
        input_tokens: 5,
        output_tokens: if preparing { 0 } else { 5 },
        microusd: 0,
    };
    for i in 0..if preparing { 2 } else { 1 } {
        results
            .dispatch(
                &mut budget,
                contract["task_id"].as_str().unwrap(),
                &format!("call-{i}"),
                &c::digest(b"fake-input"),
                Allowance {
                    input_tokens: 10,
                    output_tokens: 10,
                    microusd: 0,
                },
                || Ok((serde_json::json!({"fake":true}), usage)),
            )
            .unwrap();
    }
    let audit = results.audit(&mut budget).unwrap();
    if preparing {
        exclusive_json(
            Path::new("receipt.json"),
            &c::preparation::PreparationReceipt {
                version: 1,
                snapshot_sha256: c::digest(b"snapshot"),
                audit,
            },
        )
        .unwrap();
    } else {
        let mut observation: Observation =
            bounded_json(Path::new("../../observation.json")).unwrap();
        observation.work = serde_json::from_value(request["work"].clone()).unwrap();
        observation.result.item_id = observation.work.case_id.clone();
        observation.provider_audit = Some(audit);
        exclusive_json(Path::new("observation.json"), &observation).unwrap();
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open("../phase-count")
        .unwrap()
        .write_all(if preparing { b"p" } else { b"t" })
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn preparation_is_budgeted_before_tasks_and_verified_without_rerunning_on_resume() {
    use std::os::unix::fs::PermissionsExt;
    let root = Scratch::new();
    let (mut manifest, mut observation) = fixture();
    // This verifies journal/phase accounting, not startup latency. Loading the
    // large test executable alongside required-database suites can exceed the
    // tiny default fixture timeout. The separate deadline test keeps its 1s cap.
    manifest.limits.task_seconds = 30;
    manifest.provider_budget = Some(c::ProviderBudget {
        max_calls_per_task: 1,
    });
    manifest.preparation_budget = Some(c::Limits {
        requests: 2,
        tokens: 100,
        microusd: 0,
        task_seconds: 30,
    });
    // Preparation consumes two requests and ten tokens; exactly two task
    // requests remain. Unused preparation tokens are released after verification.
    manifest.limits.requests = 4;
    manifest.limits.tokens = 225;
    for asset in &manifest.assets {
        std::fs::write(root.0.join(&asset.path), b"fixture").unwrap();
    }
    let driver = root.0.join("driver.sh");
    let binary = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .replace('\'', "'\\''");
    std::fs::write(&driver, format!("#!/bin/sh\nMEMORY_R3_PREPARATION_TEST_REQUEST=\"$1\" exec '{binary}' --exact commands::eval_campaign::tests::preparation::driver_child\n")).unwrap();
    std::fs::set_permissions(&driver, std::fs::Permissions::from_mode(0o700)).unwrap();
    let driver_hash = c::digest(&std::fs::read(&driver).unwrap());
    manifest.assets.last_mut().unwrap().sha256 = driver_hash.clone();
    observation.isolation.driver_sha256 = driver_hash.clone();
    exclusive_json(&root.0.join("observation.json"), &observation).unwrap();
    std::fs::create_dir(root.0.join("suite")).unwrap();
    let items = manifest.cases.iter().map(|case| serde_json::to_string(&if case.track == c::Track::Knowledge {
        serde_json::json!({"eval_type":"grounded_answer","id":case.id,"question":"fixture"})
    } else { serde_json::json!({"eval_type":"agent_build_sequence","id":case.id,"fixture":".","agent_command":"false",
        "steps":(1..=3).map(|i| serde_json::json!({"id":format!("step-{i}"),"prompt":"fixture"})).collect::<Vec<_>>()}) }).unwrap()).collect::<Vec<_>>().join("\n");
    for (name, bytes) in [
        (
            "suite/suite.toml",
            "name = \"fixture\"\nitems = \"items.jsonl\"\n".to_string(),
        ),
        ("suite/items.jsonl", items),
    ] {
        std::fs::write(root.0.join(name), &bytes).unwrap();
        manifest.assets.push(c::Asset {
            path: name.into(),
            sha256: c::digest(bytes.as_bytes()),
            kind: c::AssetKind::Labels,
        });
    }
    let manifest_path = root.0.join("campaign.json");
    exclusive_json(&manifest_path, &manifest).unwrap();
    let mut plan = manifest.plan(Split::Development).unwrap();
    plan.work.truncate(2);
    let out = root.0.join("out");
    std::fs::create_dir(&out).unwrap();
    let mut artifact = Artifact {
        manifest_sha256: plan.manifest_sha256.clone(),
        split: Split::Development,
        offline: true,
        completed: false,
        preparation_microusd: None,
        preparation_receipt: None,
        approvals: vec![],
        observations: vec![],
    };
    let mut ledger = Ledger::default();
    execute(
        &manifest_path,
        &manifest,
        &driver,
        &driver_hash,
        &out,
        &plan,
        true,
        false,
        &mut ledger,
        &mut artifact,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(out.join("phase-count")).unwrap(), b"ptt");
    assert_eq!(
        artifact
            .preparation_receipt
            .as_ref()
            .unwrap()
            .audit
            .ledger
            .tokens,
        10
    );
    assert_eq!(ledger.tokens, 20);
    artifact.observations.clear();
    execute(
        &manifest_path,
        &manifest,
        &driver,
        &driver_hash,
        &out,
        &plan,
        true,
        true,
        &mut ledger,
        &mut artifact,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(out.join("phase-count")).unwrap(), b"ptt");
    assert_eq!(artifact.observations.len(), 2);
    let mut third = plan.clone();
    third
        .work
        .push(manifest.plan(Split::Development).unwrap().work[2].clone());
    artifact.observations.clear();
    assert!(
        execute(
            &manifest_path,
            &manifest,
            &driver,
            &driver_hash,
            &out,
            &third,
            true,
            true,
            &mut ledger,
            &mut artifact
        )
        .await
        .is_err()
    );
    assert_eq!(std::fs::read(out.join("phase-count")).unwrap(), b"ptt");
    // An interrupted preparation has no acceptance receipt: never rerun it.
    std::fs::rename(
        out.join("preparation/accepted.json"),
        out.join("preparation/accepted.saved"),
    )
    .unwrap();
    artifact.observations.clear();
    assert!(
        execute(
            &manifest_path,
            &manifest,
            &driver,
            &driver_hash,
            &out,
            &plan,
            true,
            true,
            &mut ledger,
            &mut artifact
        )
        .await
        .is_err()
    );
    assert_eq!(std::fs::read(out.join("phase-count")).unwrap(), b"ptt");
    // A syntactically valid changed receipt is not a replacement for acceptance.
    std::fs::rename(
        out.join("preparation/accepted.saved"),
        out.join("preparation/accepted.json"),
    )
    .unwrap();
    let receipt_path = out.join("preparation/receipt.json");
    let mut receipt: c::preparation::PreparationReceipt = bounded_json(&receipt_path).unwrap();
    receipt.snapshot_sha256 = c::digest(b"changed");
    std::fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    assert!(
        execute(
            &manifest_path,
            &manifest,
            &driver,
            &driver_hash,
            &out,
            &plan,
            true,
            true,
            &mut ledger,
            &mut artifact
        )
        .await
        .is_err()
    );
    assert_eq!(std::fs::read(out.join("phase-count")).unwrap(), b"ptt");

    // Process death after the durable reservation cannot dispatch a first task
    // or turn explicit resume into an automatic repeated preparation call.
    std::fs::write(root.0.join("interrupt-preparation"), b"synthetic").unwrap();
    let interrupted = root.0.join("interrupted");
    std::fs::create_dir(&interrupted).unwrap();
    artifact.preparation_receipt = None;
    assert!(
        execute(
            &manifest_path,
            &manifest,
            &driver,
            &driver_hash,
            &interrupted,
            &plan,
            true,
            false,
            &mut ledger,
            &mut artifact
        )
        .await
        .is_err()
    );
    assert!(!interrupted.join("work-00000").exists());
    let journal = interrupted.join("preparation/provider/budget.jsonl");
    let bytes = std::fs::read(&journal).unwrap();
    assert!(
        execute(
            &manifest_path,
            &manifest,
            &driver,
            &driver_hash,
            &interrupted,
            &plan,
            true,
            true,
            &mut ledger,
            &mut artifact
        )
        .await
        .is_err()
    );
    assert_eq!(std::fs::read(journal).unwrap(), bytes);
}
