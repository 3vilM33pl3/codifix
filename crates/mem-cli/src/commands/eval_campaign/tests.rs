// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

#[tokio::test]
async fn paid_dispatch_is_rejected_before_reading_files_or_starting_processes() {
    let error = handle(EvalCampaignCommand::Run {
        manifest: "must-not-read".into(),
        driver: "must-not-execute".into(),
        out: "must-not-create".into(),
        split: "held-out".into(),
        allow_shell: true,
        offline: false,
        approvals: vec![],
    })
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("paid campaign execution remains unavailable")
    );
}

#[tokio::test]
async fn offline_driver_requires_explicit_shell_consent() {
    let error = handle(EvalCampaignCommand::Run {
        manifest: "must-not-read".into(),
        driver: "must-not-execute".into(),
        out: "must-not-create".into(),
        split: "development".into(),
        allow_shell: false,
        offline: true,
        approvals: vec![],
    })
    .await
    .unwrap_err();
    assert!(error.to_string().contains("--allow-shell"));
}

#[test]
fn evidence_files_cannot_be_overwritten() {
    let root = std::env::temp_dir().join(format!("memory-r3-evidence-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("receipt.json");
    exclusive_json(&path, &1).unwrap();
    assert!(exclusive_json(&path, &2).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "1");
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(root).unwrap();
    assert!(split("heldout").is_err());
    assert_eq!(split("held-out").unwrap(), Split::HeldOut);
}

fn fixture() -> (Manifest, Observation) {
    let mut cases = Vec::new();
    for (track, split, count, size) in [
        (c::Track::Knowledge, Split::Development, 10, 4),
        (c::Track::Knowledge, Split::HeldOut, 25, 4),
        (c::Track::Coding, Split::Development, 4, 1),
        (c::Track::Coding, Split::HeldOut, 12, 1),
    ] {
        for cluster in 0..count {
            for item in 0..size {
                let cluster = format!("{track:?}-{split:?}-{cluster}");
                cases.push(c::Case {
                    id: format!("{cluster}-{item}"),
                    cluster,
                    track,
                    split,
                    suite: "suite".into(),
                    sources: vec!["source.md".into()],
                });
            }
        }
    }
    let model = c::Model {
        provider: "fake".into(),
        resolved_id: "fake-v1".into(),
        reasoning: "none".into(),
        embedding_id: "fake-vector".into(),
        max_input_tokens: 100,
        max_output_tokens: 100,
        input_microusd_per_million: 0,
        output_microusd_per_million: 0,
    };
    let manifest = Manifest {
        version: 1,
        name: "fixture".into(),
        semantic_time: "2026-09-10T00:00:00Z".parse().unwrap(),
        seed: 1,
        repeats: 3,
        label_status: "draft".into(),
        smoke_only: true,
        model: model.clone(),
        limits: c::Limits {
            requests: 10000,
            tokens: 10000000,
            microusd: 0,
            task_seconds: 5,
        },
        reference_baseline: None,
        assets: [
            ("source.md", c::AssetKind::Source),
            ("labels.json", c::AssetKind::Labels),
            ("fixture.rs", c::AssetKind::Fixture),
            ("driver.sh", c::AssetKind::Configuration),
        ]
        .into_iter()
        .map(|(name, kind)| c::Asset {
            path: name.into(),
            sha256: c::digest(b"fixture"),
            kind,
        })
        .collect(),
        cases,
    };
    let work = manifest.plan(Split::Development).unwrap().work[0].clone();
    let observation = Observation {
        work: work.clone(),
        model,
        result: mem_eval::EvalItemResult {
            item_id: work.case_id,
            eval_type: "fixture".into(),
            condition: mem_eval::EvalCondition::FullMemory,
            metadata: Default::default(),
            success: false,
            skipped: false,
            scores: Default::default(),
            duration_ms: Some(1),
            token_usage: Some(mem_record::TokenUsage {
                input_tokens: 5,
                output_tokens: 5,
                total_tokens: 10,
                ..Default::default()
            }),
            answer: None,
            notes: vec![],
            sub_results: vec![],
        },
        execution_microusd: Some(0),
        provider_requests: 1,
        uncertain_charge: false,
        critical_failure: false,
        unresolved_rubric: false,
        isolation: c::Isolation {
            before_sha256: c::digest(b"snapshot"),
            after_sha256: c::digest(b"snapshot"),
            snapshot_sha256: c::digest(b"snapshot"),
            configuration_sha256: c::digest(b"config"),
            driver_sha256: c::digest(b"fixture"),
            service_build: "fake-build".into(),
            semantic_time: manifest.semantic_time,
            sealed: true,
            restricted_network: true,
            hidden_tests_inaccessible: true,
        },
    };
    (manifest, observation)
}

#[test]
fn invalid_measurement_stops_but_negative_quality_is_valid_evidence() {
    let (manifest, observation) = fixture();
    let check = |o: &Observation| {
        validate_task_evidence(&manifest, &observation.work, o, Some(&observation))
    };
    assert!(
        check(&observation).is_ok(),
        "a wrong answer must remain measurable"
    );
    let mutations: Vec<fn(&mut Observation)> = vec![
        |o| o.model.resolved_id = "another-model".into(),
        |o| o.result.item_id = "other-task".into(),
        |o| o.result.skipped = true,
        |o| o.result.duration_ms = None,
        |o| o.result.token_usage = None,
        |o| o.result.token_usage.as_mut().unwrap().total_tokens = 1,
        |o| {
            let u = o.result.token_usage.as_mut().unwrap();
            u.input_tokens = 101;
            u.total_tokens = 106;
        },
        |o| {
            let u = o.result.token_usage.as_mut().unwrap();
            u.output_tokens = 101;
            u.total_tokens = 106;
        },
        |o| o.uncertain_charge = true,
        |o| o.unresolved_rubric = true,
        |o| o.isolation.after_sha256 = c::digest(b"changed"),
        |o| o.isolation.configuration_sha256 = c::digest(b"different-config"),
        |o| o.isolation.service_build = "different-build".into(),
    ];
    for mutate in mutations {
        let mut invalid = observation.clone();
        mutate(&mut invalid);
        assert!(check(&invalid).is_err());
    }
}

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("memory-r3-runner-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn final_driver_cannot_mutate_locked_inputs_or_rewrite_the_manifest() {
    use std::os::unix::fs::PermissionsExt;
    for command in [
        "printf changed > ../../source.md",
        "printf '{}' > ../../campaign.json",
        "sed -i 's/\"seed\": 1,/\"seed\": 2,/' ../../campaign.json",
    ] {
        let root = Scratch::new();
        let (mut manifest, _) = fixture();
        for asset in &manifest.assets {
            std::fs::write(root.0.join(&asset.path), b"fixture").unwrap();
        }
        let driver = root.0.join("driver.sh");
        std::fs::write(&driver, format!("#!/bin/sh\nset -eu\n{command}\n")).unwrap();
        std::fs::set_permissions(&driver, std::fs::Permissions::from_mode(0o700)).unwrap();
        let driver_hash = c::digest(&std::fs::read(&driver).unwrap());
        manifest.assets.last_mut().unwrap().sha256 = driver_hash.clone();
        std::fs::create_dir(root.0.join("suite")).unwrap();
        let items = manifest.cases.iter().map(|case| {
            let item = if case.track == c::Track::Knowledge {
                serde_json::json!({"eval_type":"grounded_answer","id":case.id,"question":"fixture"})
            } else {
                serde_json::json!({"eval_type":"agent_build_sequence","id":case.id,
                    "fixture":".","agent_command":"false","steps":(1..=3).map(|step|
                        serde_json::json!({"id":format!("step-{step}"),"prompt":"fixture"})).collect::<Vec<_>>()})
            };
            serde_json::to_string(&item).unwrap()
        }).collect::<Vec<_>>().join("\n");
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
        let path = root.0.join("campaign.json");
        exclusive_json(&path, &manifest).unwrap();
        let mut plan = manifest.plan(Split::Development).unwrap();
        verify_campaign_inputs(&path, &plan.manifest_sha256).unwrap();
        plan.work.truncate(1); // One final dispatch; no subsequent iteration to detect mutation.
        let out = root.0.join("out");
        std::fs::create_dir(&out).unwrap();
        let mut artifact = Artifact {
            manifest_sha256: plan.manifest_sha256.clone(),
            split: Split::Development,
            offline: true,
            completed: false,
            preparation_microusd: None,
            approvals: vec![],
            observations: vec![],
        };
        let mut ledger = Ledger::default();
        assert!(
            execute(
                &path,
                &manifest,
                &driver,
                &driver_hash,
                &out,
                &plan,
                true,
                &mut ledger,
                &mut artifact
            )
            .await
            .is_err()
        );
        assert_eq!(ledger.requests, 1);
        assert!(artifact.observations.is_empty());
        assert!(out.join("work-00000/reservation.json").exists());
        assert!(verify_campaign_inputs(&path, &plan.manifest_sha256).is_err());
    }
}
