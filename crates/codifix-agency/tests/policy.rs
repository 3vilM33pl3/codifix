use chrono::{DateTime, Duration, Utc};
use codifix_agency::*;
use std::collections::BTreeSet;
use uuid::Uuid;
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-31T12:00:00Z")
        .expect("timestamp")
        .with_timezone(&Utc)
}
fn set<T: Ord>(items: impl IntoIterator<Item = T>) -> BTreeSet<T> {
    items.into_iter().collect()
}
fn mandate() -> Mandate {
    Mandate {
        id: Uuid::new_v4(),
        parent_id: None,
        issuer_instance_id: Uuid::new_v4(),
        subject_instance_id: Uuid::new_v4(),
        specialist_id: "research".into(),
        capabilities: set(["memory.read".into()]),
        resources: set([ResourceScope {
            kind: "project".into(),
            id: "p1".into(),
        }]),
        destinations: set([ModelDestination::Local {
            runtime: "ollama".into(),
        }]),
        expires_at: now() + Duration::hours(1),
        status: MandateStatus::Active,
        budget: Budget {
            micro_usd: 100,
            calls: 5,
            turns: 10,
            delegation_depth: 2,
        },
    }
}
fn child(parent: &Mandate) -> Mandate {
    let mut c = parent.clone();
    c.id = Uuid::new_v4();
    c.parent_id = Some(parent.id);
    c.issuer_instance_id = parent.subject_instance_id;
    c.budget.delegation_depth = 1;
    c
}
fn task() -> TaskContract {
    TaskContract {
        id: Uuid::new_v4(),
        mandate_id: Uuid::new_v4(),
        specialist_id: "research".into(),
        objective: "Find evidence".into(),
        acceptance_criteria: vec!["cite primary source".into()],
        revision: 0,
        status: TaskStatus::Submitted,
        clarification: None,
        plan_revision: None,
        outcome: None,
        updated_at: now(),
    }
}
#[test]
fn attenuation_denies_each_expansion_and_spent_parent_budget() {
    let p = mandate();
    let c = child(&p);
    p.validate_child(&c, Usage::default(), now())
        .expect("attenuated");
    let mut variants = vec![];
    let mut x = c.clone();
    x.capabilities.insert("memory.write".into());
    variants.push(x);
    let mut x = c.clone();
    x.resources.insert(ResourceScope {
        kind: "project".into(),
        id: "p2".into(),
    });
    variants.push(x);
    let mut x = c.clone();
    x.destinations.insert(ModelDestination::Cloud {
        provider: "cloud".into(),
        region: "uk".into(),
    });
    variants.push(x);
    let mut x = c.clone();
    x.expires_at += Duration::seconds(1);
    variants.push(x);
    let mut x = c.clone();
    x.budget.delegation_depth = 2;
    variants.push(x);
    let mut x = c.clone();
    x.issuer_instance_id = Uuid::new_v4();
    variants.push(x);
    for x in variants {
        assert!(p.validate_child(&x, Usage::default(), now()).is_err());
    }
    assert!(
        p.validate_child(
            &c,
            Usage {
                calls: 1,
                ..Usage::default()
            },
            now()
        )
        .is_err()
    );
}
#[test]
fn expiry_pause_revocation_and_overflow_fail_closed() {
    let mut p = mandate();
    assert!(p.validate(p.expires_at).is_err());
    p.pause().expect("pause");
    assert!(p.validate(now()).is_err());
    p.resume(now()).expect("resume");
    p.revoke();
    assert!(p.resume(now()).is_err());
    assert!(
        p.validate_child(&child(&p), Usage::default(), now())
            .is_err()
    );
    assert!(
        Usage {
            micro_usd: u64::MAX,
            ..Usage::default()
        }
        .checked_add(Usage {
            micro_usd: 1,
            ..Usage::default()
        })
        .is_err()
    );
}
#[test]
fn resources_are_exact_and_no_wildcards_are_inferred() {
    let p = mandate();
    let resource = ResourceScope {
        kind: "project".into(),
        id: "p1/child".into(),
    };
    assert!(
        p.authorize("memory.read", &resource, None, Usage::default(), now())
            .is_err()
    );
    let resource = p.resources.first().expect("resource");
    p.authorize("memory.read", resource, None, Usage::default(), now())
        .expect("allowed");
    assert!(
        p.authorize(
            "memory.read",
            resource,
            None,
            Usage {
                calls: 6,
                ..Usage::default()
            },
            now()
        )
        .is_err()
    );
}
#[test]
fn task_requires_answer_plan_acceptance_and_complete_evidence() {
    let mut t = task();
    t.transition(TaskStatus::Working, now()).expect("start");
    t.clarify("Which version?".into(), now()).expect("clarify");
    assert!(t.transition(TaskStatus::Working, now()).is_err());
    t.answer_clarification("v1".into(), now()).expect("answer");
    t.revise_plan(
        "Narrow evidence".into(),
        vec!["Read specification".into()],
        now(),
    )
    .expect("revise");
    assert!(t.transition(TaskStatus::Working, now()).is_err());
    t.accept_plan(now()).expect("accept");
    let mut outcome = TaskOutcome {
        summary: "Found".into(),
        artifact_refs: vec![],
        evidence_refs: vec!["source:1".into()],
        acceptance: vec![],
    };
    assert!(t.finish(outcome.clone(), now()).is_err());
    outcome.acceptance.push(AcceptanceResult {
        criterion: "cite primary source".into(),
        passed: true,
        evidence_ref: "source:1".into(),
    });
    t.finish(outcome, now()).expect("finish");
    assert!(t.transition(TaskStatus::Working, now()).is_err());
}
#[test]
fn task_time_cannot_move_back_and_wire_rejects_unknown_authority() {
    let mut t = task();
    assert!(
        t.transition(TaskStatus::Working, now() - Duration::seconds(1))
            .is_err()
    );
    let mut wire = serde_json::to_value(mandate()).expect("json");
    wire["admin"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Mandate>(wire).is_err());
}
