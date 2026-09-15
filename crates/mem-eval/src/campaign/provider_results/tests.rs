// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::campaign::Limits;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("r3-results-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn budget(&self, resume: bool) -> RequestBudget {
        if resume {
            RequestBudget::resume_scoped(
                &self.0.join("journal"),
                &digest(b"campaign"),
                limits(),
                limits(),
            )
            .unwrap()
        } else {
            RequestBudget::create_scoped(
                &self.0.join("journal"),
                &digest(b"campaign"),
                limits(),
                limits(),
            )
            .unwrap()
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn limits() -> Limits {
    Limits {
        requests: 5,
        tokens: 500,
        microusd: 100,
        task_seconds: 10,
    }
}
fn allowance() -> Allowance {
    Allowance {
        input_tokens: 20,
        output_tokens: 10,
        microusd: 10,
    }
}
fn usage() -> Allowance {
    Allowance {
        input_tokens: 3,
        output_tokens: 2,
        microusd: 2,
    }
}

#[test]
fn completed_output_is_reused_after_restart_without_another_call_or_charge() {
    let f = Fixture::new();
    let mut budget = f.budget(false);
    let results = ProviderResults::create(&f.0.join("results")).unwrap();
    let expected = serde_json::json!({"text":"PRIVATE_SYNTHETIC_OUTPUT"});
    let output = results
        .dispatch(
            &mut budget,
            "task",
            "request",
            &digest(b"input"),
            allowance(),
            || Ok((expected.clone(), usage())),
        )
        .unwrap();
    assert_eq!(output, expected);
    assert!(
        !fs::read_to_string(f.0.join("journal"))
            .unwrap()
            .contains("PRIVATE_SYNTHETIC_OUTPUT")
    );
    drop(budget);
    let mut budget = f.budget(true);
    let reused = results
        .dispatch(
            &mut budget,
            "task",
            "request",
            &digest(b"input"),
            allowance(),
            || panic!("must reuse completed output"),
        )
        .unwrap();
    assert_eq!(reused, expected);
    assert_eq!(budget.ledger().requests, 1);
    assert_eq!(budget.ledger().microusd, 2);
    assert!(
        results
            .dispatch(
                &mut budget,
                "other-task",
                "request",
                &digest(b"input"),
                allowance(),
                || panic!()
            )
            .is_err()
    );
    assert!(
        results
            .dispatch(
                &mut budget,
                "task",
                "request",
                &digest(b"changed"),
                allowance(),
                || panic!()
            )
            .is_err()
    );
}

#[test]
fn absent_corrupt_and_changed_receipts_do_not_repeat_provider_or_refund() {
    for content in [None, Some(b"{partial".as_slice())] {
        let f = Fixture::new();
        let mut budget = f.budget(false);
        let results = ProviderResults::create(&f.0.join("results")).unwrap();
        let err = results
            .dispatch(
                &mut budget,
                "task",
                "request",
                &digest(b"input"),
                allowance(),
                || anyhow::bail!("SECRET_PROVIDER_BODY"),
            )
            .unwrap_err();
        assert!(!err.to_string().contains("SECRET"));
        if let Some(bytes) = content {
            results.persist("request", bytes).unwrap();
        }
        drop(budget);
        let mut budget = f.budget(true);
        assert!(
            results
                .dispatch(
                    &mut budget,
                    "task",
                    "request",
                    &digest(b"input"),
                    allowance(),
                    || panic!()
                )
                .is_err()
        );
        assert!(results.recover(&mut budget, "request").is_err());
        assert_eq!(budget.uncertain_ids(), ["request"]);
        assert_eq!(budget.ledger().microusd, 10);
    }
    let f = Fixture::new();
    let mut budget = f.budget(false);
    let results = ProviderResults::create(&f.0.join("results")).unwrap();
    results
        .dispatch(
            &mut budget,
            "task",
            "request",
            &digest(b"input"),
            allowance(),
            || Ok((Value::Bool(true), usage())),
        )
        .unwrap();
    let path = results.path("request");
    let mut receipt: Receipt = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    receipt.output = Value::Bool(false);
    fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    assert!(results.recover(&mut budget, "request").is_err());
    assert_eq!(budget.ledger().requests, 1);
}

#[test]
fn result_crash_child() {
    let Some(root) = std::env::var_os("MEMORY_R3_RESULT_CRASH") else {
        return;
    };
    let root = PathBuf::from(root);
    let mut budget = RequestBudget::create_scoped(
        &root.join("journal"),
        &digest(b"campaign"),
        limits(),
        limits(),
    )
    .unwrap();
    let results = ProviderResults::create(&root.join("results")).unwrap();
    budget
        .reserve_for_task("task", "request", &digest(b"input"), allowance())
        .unwrap();
    let receipt = Receipt {
        version: 1,
        campaign_sha256: budget.identity().into(),
        task_id: "task".into(),
        request_id: "request".into(),
        payload_sha256: digest(b"input"),
        allowance: allowance(),
        usage: usage(),
        output: Value::String("persisted-before-crash".into()),
    };
    results
        .persist("request", &serde_json::to_vec(&receipt).unwrap())
        .unwrap();
    std::process::exit(17); // No settlement and no destructors.
}

#[test]
fn complete_result_before_crash_recovers_explicitly_without_provider_execution() {
    let f = Fixture::new();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "campaign::provider_results::tests::result_crash_child",
        ])
        .env("MEMORY_R3_RESULT_CRASH", &f.0)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(17));
    let results = ProviderResults::open(&f.0.join("results")).unwrap();
    let mut budget = f.budget(true);
    assert_eq!(budget.uncertain_ids(), ["request"]);
    assert!(
        results
            .dispatch(
                &mut budget,
                "task",
                "request",
                &digest(b"input"),
                allowance(),
                || panic!()
            )
            .is_err()
    );
    assert_eq!(
        results.recover(&mut budget, "request").unwrap(),
        Value::String("persisted-before-crash".into())
    );
    assert!(budget.uncertain_ids().is_empty());
    assert_eq!(budget.ledger().tokens, 5);
    assert_eq!(budget.task_ledger("task").unwrap().microusd, 2);
}

#[test]
fn foreign_campaign_and_excess_usage_cannot_be_reconciled() {
    let f = Fixture::new();
    let results = ProviderResults::create(&f.0.join("results")).unwrap();
    let mut budget = f.budget(false);
    budget
        .reserve_for_task("task", "request", &digest(b"input"), allowance())
        .unwrap();
    let receipt = Receipt {
        version: 1,
        campaign_sha256: digest(b"other-campaign"),
        task_id: "task".into(),
        request_id: "request".into(),
        payload_sha256: digest(b"input"),
        allowance: allowance(),
        usage: usage(),
        output: Value::Null,
    };
    results
        .persist("request", &serde_json::to_vec(&receipt).unwrap())
        .unwrap();
    assert!(results.recover(&mut budget, "request").is_err());
    assert_eq!(budget.uncertain_ids(), ["request"]);
    let f = Fixture::new();
    let mut budget = f.budget(false);
    let results = ProviderResults::create(&f.0.join("results")).unwrap();
    let excessive = Allowance {
        output_tokens: 11,
        ..usage()
    };
    assert!(
        results
            .dispatch(
                &mut budget,
                "task",
                "request",
                &digest(b"input"),
                allowance(),
                || Ok((Value::Null, excessive))
            )
            .is_err()
    );
    assert!(!results.path("request").exists());
    assert_eq!(budget.ledger().tokens, 30);
}
