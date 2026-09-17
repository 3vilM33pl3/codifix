use mem_supervisor::{
    Area, BudgetDay, DAILY_AREA_MODEL_CALLS, DAILY_AREA_USD_MICROS, DEFAULT_MODEL, StableId,
    UnixSeconds, UsdMicros,
    action::{Action, ActionError, ActionKey, ActionKind, ActionState, KnownOutcome, StopReason},
    budget::{Admission, AttemptState, BudgetError, BudgetLedger, ProviderRequest, Settlement},
    lease::{LeaseAuthority, LeaseError},
    policy::{
        PolicyError, QuestionDenial, QuestionScope, StandingQuestionPolicy, SupervisorPolicy,
    },
    upgrade::{
        Architecture, BackupEvidence, LocalSignature, LondonLocalTime, ReleaseArtifact,
        ReleaseManifest, UpgradeBlocker, UpgradeContext, evaluate_upgrade,
    },
};

fn id(value: &str) -> StableId {
    StableId::new(value).expect("valid test ID")
}

fn policy() -> SupervisorPolicy {
    let mut policy = SupervisorPolicy::new(Area::Homelab, id("policy-1"));
    policy.set_enabled(true);
    policy
}

fn request(value: &str, cost: u64) -> ProviderRequest {
    ProviderRequest {
        id: id(value),
        action_id: id("action-1"),
        policy_revision: id("policy-1"),
        model: id(DEFAULT_MODEL),
        worst_case_cost: UsdMicros(cost),
    }
}

fn ledger() -> BudgetLedger {
    BudgetLedger::new(Area::Homelab, BudgetDay(0))
}

#[test]
fn policies_default_disabled_and_cannot_exceed_partition_caps() {
    let policy = SupervisorPolicy::new(Area::Homelab, id("policy-1"));
    assert_eq!(
        policy.validate(Area::Homelab, &id("policy-1")),
        Err(PolicyError::Disabled)
    );
    assert_eq!(policy.model(), "gpt-5.6-sol");
    assert_eq!(policy.daily_limit(), UsdMicros(2_500_000));
    assert_eq!(policy.daily_calls(), 20);
    assert_eq!(
        policy.clone().with_limits(UsdMicros(2_500_001), 20),
        Err(PolicyError::AboveHardCeiling)
    );
    assert_eq!(
        policy.with_limits(UsdMicros(2_500_000), 21),
        Err(PolicyError::AboveHardCeiling)
    );
    assert_eq!(DAILY_AREA_USD_MICROS * 4, 10_000_000);
}

#[test]
fn duplicate_attempt_does_not_reserve_or_send_twice() {
    let mut ledger = ledger();
    let policy = policy();
    let request = request("attempt-1", 100_000);
    assert_eq!(
        ledger.reserve(&policy, request.clone(), UnixSeconds(1)),
        Ok(Admission::Reserved)
    );
    assert_eq!(
        ledger.reserve(&policy, request.clone(), UnixSeconds(1)),
        Ok(Admission::Existing(AttemptState::Reserved))
    );
    assert_eq!(ledger.totals().calls, 1);
    assert_eq!(ledger.totals().reserved_usd_micros, 100_000);
    ledger
        .begin(&request.id, &policy, UnixSeconds(2))
        .expect("first send");
    assert_eq!(
        ledger.begin(&request.id, &policy, UnixSeconds(2)),
        Err(BudgetError::InvalidTransition)
    );
    let mut changed = request.clone();
    changed.action_id = id("different-action");
    assert_eq!(
        ledger.reserve(&policy, changed, UnixSeconds(2)),
        Err(BudgetError::IdempotencyConflict)
    );
    assert_eq!(
        ledger.reserve(&policy, request, UnixSeconds(2)),
        Ok(Admission::Existing(AttemptState::InFlight))
    );
}

#[test]
fn exact_cost_limit_accepts_boundary_and_denies_one_micro_over() {
    let mut ledger = ledger();
    let policy = policy();
    ledger
        .reserve(
            &policy,
            request("a", DAILY_AREA_USD_MICROS - 1),
            UnixSeconds(0),
        )
        .expect("fits");
    assert_eq!(
        ledger.reserve(&policy, request("b", 2), UnixSeconds(0)),
        Err(BudgetError::CostLimit)
    );
    ledger
        .reserve(&policy, request("b", 1), UnixSeconds(0))
        .expect("exact boundary");
    assert_eq!(
        ledger.totals().reserved_usd_micros,
        u128::from(DAILY_AREA_USD_MICROS)
    );
    assert_eq!(
        ledger.reserve(&policy, request("c", 1), UnixSeconds(0)),
        Err(BudgetError::CostLimit)
    );
}

#[test]
fn zero_cost_results_still_consume_call_slots() {
    let mut ledger = ledger();
    let policy = policy();
    for n in 0..DAILY_AREA_MODEL_CALLS {
        let attempt = request(&format!("attempt-{n}"), 1);
        ledger
            .reserve(&policy, attempt.clone(), UnixSeconds(0))
            .expect("call fits");
        ledger
            .begin(&attempt.id, &policy, UnixSeconds(1))
            .expect("begin");
        ledger
            .settle(&attempt.id, UsdMicros(0))
            .expect("known free result");
    }
    assert_eq!(ledger.totals().charged_usd_micros, 0);
    assert_eq!(ledger.totals().calls, 20);
    assert_eq!(
        ledger.reserve(&policy, request("overflow", 1), UnixSeconds(2)),
        Err(BudgetError::CallLimit)
    );
}

#[test]
fn uncertain_usage_retains_money_and_call_until_authoritative_settlement() {
    let mut ledger = ledger();
    let policy = policy();
    let attempt = request("a", DAILY_AREA_USD_MICROS);
    ledger
        .reserve(&policy, attempt.clone(), UnixSeconds(0))
        .expect("reserve");
    ledger
        .begin(&attempt.id, &policy, UnixSeconds(1))
        .expect("begin");
    ledger
        .mark_uncertain(&attempt.id)
        .expect("unknown response");
    ledger
        .mark_uncertain(&attempt.id)
        .expect("duplicate uncertainty");
    assert_eq!(ledger.totals().reserved_usd_micros, 2_500_000);
    assert_eq!(ledger.totals().calls, 1);
    assert_eq!(
        ledger.release_before_send(&attempt.id),
        Err(BudgetError::InvalidTransition)
    );
    assert_eq!(
        ledger.begin(&attempt.id, &policy, UnixSeconds(2)),
        Err(BudgetError::InvalidTransition)
    );
    assert_eq!(
        ledger.reserve(&policy, request("b", 1), UnixSeconds(2)),
        Err(BudgetError::CostLimit)
    );
    assert_eq!(
        ledger.settle(&attempt.id, UsdMicros(27)),
        Ok(Settlement::Settled)
    );
    assert_eq!(
        ledger.settle(&attempt.id, UsdMicros(27)),
        Ok(Settlement::AlreadySettled)
    );
    assert_eq!(
        ledger.settle(&attempt.id, UsdMicros(28)),
        Err(BudgetError::IdempotencyConflict)
    );
    assert_eq!(ledger.totals().charged_usd_micros, 27);
    assert_eq!(ledger.totals().reserved_usd_micros, 0);
    assert_eq!(ledger.totals().calls, 1);
}

#[test]
fn overrun_is_recorded_without_overflow_or_freeing_spend() {
    let mut ledger = ledger();
    let policy = policy();
    ledger
        .reserve(&policy, request("a", 1), UnixSeconds(0))
        .expect("reserve");
    ledger
        .begin(&id("a"), &policy, UnixSeconds(1))
        .expect("begin");
    assert_eq!(
        ledger.settle(&id("a"), UsdMicros(u64::MAX)),
        Ok(Settlement::ExceededReservation)
    );
    assert_eq!(ledger.totals().charged_usd_micros, u128::from(u64::MAX));
    assert_eq!(
        ledger.reserve(&policy, request("b", 1), UnixSeconds(1)),
        Err(BudgetError::CostLimit)
    );
}

#[test]
fn release_before_send_does_not_allow_id_reuse() {
    let mut ledger = ledger();
    let policy = policy();
    ledger
        .reserve(&policy, request("a", 10), UnixSeconds(0))
        .expect("reserve");
    ledger.release_before_send(&id("a")).expect("never sent");
    assert_eq!(ledger.totals().calls, 0);
    assert_eq!(ledger.totals().reserved_usd_micros, 0);
    assert_eq!(
        ledger.reserve(&policy, request("a", 10), UnixSeconds(0)),
        Ok(Admission::Existing(AttemptState::ReleasedBeforeSend))
    );
    assert_eq!(
        ledger.begin(&id("a"), &policy, UnixSeconds(1)),
        Err(BudgetError::InvalidTransition)
    );
}

#[test]
fn current_policy_is_checked_before_dispatch_and_settlement_survives_revocation() {
    let mut ledger = ledger();
    let mut policy = policy();
    ledger
        .reserve(&policy, request("a", 10), UnixSeconds(0))
        .expect("reserve");
    policy.set_enabled(false);
    assert_eq!(
        ledger.begin(&id("a"), &policy, UnixSeconds(1)),
        Err(BudgetError::Policy(PolicyError::Disabled))
    );
    policy.set_enabled(true);
    let reduced = policy
        .clone()
        .with_limits(UsdMicros(9), 20)
        .expect("lowered");
    assert_eq!(
        ledger.begin(&id("a"), &reduced, UnixSeconds(1)),
        Err(BudgetError::ReducedLimit)
    );
    ledger
        .begin(&id("a"), &policy, UnixSeconds(1))
        .expect("begin");
    policy.set_enabled(false);
    assert_eq!(
        ledger.settle(&id("a"), UsdMicros(5)),
        Ok(Settlement::Settled)
    );
}

#[test]
fn area_model_revision_and_day_are_exact_boundaries() {
    let mut ledger = ledger();
    let policy = policy();
    let mut other_model = request("other-model", 1);
    other_model.model = id("gpt-5.4");
    assert_eq!(
        ledger.reserve(&policy, other_model, UnixSeconds(0)),
        Err(BudgetError::Policy(PolicyError::ModelNotApproved))
    );
    let mut stale = request("stale", 1);
    stale.policy_revision = id("policy-0");
    assert_eq!(
        ledger.reserve(&policy, stale, UnixSeconds(0)),
        Err(BudgetError::Policy(PolicyError::StaleRevision))
    );
    let mut other_area = SupervisorPolicy::new(Area::Trust, id("policy-1"));
    other_area.set_enabled(true);
    assert_eq!(
        ledger.reserve(&other_area, request("wrong-area", 1), UnixSeconds(0)),
        Err(BudgetError::Policy(PolicyError::WrongArea))
    );
    assert_eq!(
        ledger.reserve(&policy, request("zero-bound", 0), UnixSeconds(0)),
        Err(BudgetError::MissingCostBound)
    );
    ledger
        .reserve(&policy, request("a", 1), UnixSeconds(86_399))
        .expect("old day");
    assert_eq!(
        ledger.begin(&id("a"), &policy, UnixSeconds(86_400)),
        Err(BudgetError::WrongDay)
    );
}

#[test]
fn revocation_cannot_be_undone_by_resume() {
    let mut ledger = ledger();
    let mut policy = policy();
    ledger
        .reserve(&policy, request("a", 1), UnixSeconds(0))
        .expect("reserve");
    policy.revoke();
    policy.set_enabled(true);
    assert_eq!(
        ledger.begin(&id("a"), &policy, UnixSeconds(1)),
        Err(BudgetError::Policy(PolicyError::Revoked))
    );
    let scope = question_scope();
    let mut standing = StandingQuestionPolicy::new(id("standing"), scope.clone());
    standing.revoke();
    standing.set_enabled(true);
    assert_eq!(
        standing.authorize(&scope, "Question?".into(), &id("standing")),
        Err(QuestionDenial::Revoked)
    );
}

#[test]
fn leases_reject_duplicate_stale_revoked_and_cross_action_tokens() {
    let mut authority = LeaseAuthority::new(id("action-a"));
    let first = authority
        .claim(id("worker"), UnixSeconds(100), 10)
        .expect("claim");
    assert_eq!(
        authority.claim(id("worker"), UnixSeconds(100), 10),
        Err(LeaseError::AlreadyClaimed)
    );
    assert_eq!(
        authority.validate(&first, UnixSeconds(110)),
        Err(LeaseError::Expired)
    );
    let second = authority
        .claim(id("worker"), UnixSeconds(110), 10)
        .expect("reclaim expired lease");
    assert!(second.generation() > first.generation());
    assert_eq!(
        authority.validate(&first, UnixSeconds(111)),
        Err(LeaseError::StaleFence)
    );
    let mut other = LeaseAuthority::new(id("action-b"));
    let foreign = other
        .claim(id("worker"), UnixSeconds(110), 10)
        .expect("other action");
    assert_eq!(
        authority.validate(&foreign, UnixSeconds(111)),
        Err(LeaseError::StaleFence)
    );
    authority.revoke().expect("revoke");
    assert_eq!(
        authority.validate(&second, UnixSeconds(111)),
        Err(LeaseError::StaleFence)
    );
}

#[test]
fn renew_invalidates_old_token_and_overflow_does_not_mutate_authority() {
    let mut authority = LeaseAuthority::new(id("action"));
    assert_eq!(
        authority.claim(id("worker"), UnixSeconds(u64::MAX), 1),
        Err(LeaseError::Overflow)
    );
    let first = authority
        .claim(id("worker"), UnixSeconds(100), 10)
        .expect("claim");
    let renewed = authority
        .renew(&first, UnixSeconds(105), 10)
        .expect("renew");
    assert_eq!(
        authority.validate(&first, UnixSeconds(106)),
        Err(LeaseError::StaleFence)
    );
    assert_eq!(authority.validate(&renewed, UnixSeconds(114)), Ok(()));
    assert_eq!(
        authority.renew(&renewed, UnixSeconds(115), 10),
        Err(LeaseError::Expired)
    );
}

fn action() -> Action {
    Action::observe(
        id("action-1"),
        ActionKey {
            area: Area::Homelab,
            source_event: id("event-1"),
            kind: ActionKind::HealthCheck,
            policy_revision: id("policy-1"),
        },
        id("observation-1"),
    )
}

#[test]
fn action_lifecycle_rejects_skips_and_duplicate_claims() {
    let mut action = action();
    let policy = policy();
    assert_eq!(
        action.authorize(&policy),
        Err(ActionError::InvalidTransition)
    );
    action.propose().expect("propose");
    action.authorize(&policy).expect("authorize");
    action.queue(&policy).expect("queue");
    let lease = action
        .claim(&policy, id("worker"), UnixSeconds(1), 10)
        .expect("claim");
    assert_eq!(
        action.claim(&policy, id("worker"), UnixSeconds(1), 10),
        Err(ActionError::InvalidTransition)
    );
    action
        .validate_effect(&lease, &policy, UnixSeconds(2))
        .expect("effect gate");
    action
        .finish(&lease, UnixSeconds(2), KnownOutcome::Verified, id("result"))
        .expect("verified");
    assert_eq!(action.state(), ActionState::Verified);
    assert_eq!(action.evidence(), &[id("observation-1"), id("result")]);
    assert_eq!(
        action.finish(
            &lease,
            UnixSeconds(2),
            KnownOutcome::Verified,
            id("duplicate")
        ),
        Err(ActionError::InvalidTransition)
    );
}

#[test]
fn interrupted_actions_are_uncertain_and_cannot_be_blindly_retried() {
    for cancellation in [false, true] {
        let mut action = action();
        let policy = policy();
        action.propose().expect("propose");
        action.authorize(&policy).expect("authorize");
        action.queue(&policy).expect("queue");
        let lease = action
            .claim(&policy, id("worker"), UnixSeconds(1), 10)
            .expect("claim");
        if cancellation {
            action
                .stop(StopReason::Cancelled, id("cancel"))
                .expect("cancel");
        } else {
            assert_eq!(
                action.expire_lease(UnixSeconds(10), id("early")),
                Err(ActionError::LeaseStillLive)
            );
            action
                .expire_lease(UnixSeconds(11), id("expired"))
                .expect("expired");
        }
        assert_eq!(action.state(), ActionState::Uncertain);
        assert_eq!(action.queue(&policy), Err(ActionError::InvalidTransition));
        assert_eq!(
            action.claim(&policy, id("other"), UnixSeconds(12), 10),
            Err(ActionError::InvalidTransition)
        );
        assert_eq!(
            action.finish(&lease, UnixSeconds(12), KnownOutcome::Verified, id("stale")),
            Err(ActionError::InvalidTransition)
        );
        action
            .reconcile(KnownOutcome::Verified, id("authoritative-receipt"))
            .expect("reconcile");
        assert_eq!(action.state(), ActionState::Verified);
    }
}

#[test]
fn effect_admission_detects_policy_revocation_after_claim() {
    let mut action = action();
    let mut policy = policy();
    action.propose().expect("propose");
    action.authorize(&policy).expect("authorize");
    action.queue(&policy).expect("queue");
    let lease = action
        .claim(&policy, id("worker"), UnixSeconds(1), 10)
        .expect("claim");
    policy.set_enabled(false);
    assert_eq!(
        action.validate_effect(&lease, &policy, UnixSeconds(2)),
        Err(ActionError::Policy(PolicyError::Disabled))
    );
    action
        .stop(StopReason::Denied, id("revocation"))
        .expect("revoke running");
    assert_eq!(action.state(), ActionState::Uncertain);
}

fn question_scope() -> QuestionScope {
    QuestionScope {
        requester: id("homelab"),
        responder: id("workshop"),
        collection: id("releases"),
        purpose: id("maintenance"),
        onward_destination: id("local-operator"),
        model: id(DEFAULT_MODEL),
    }
}

#[test]
fn standing_questions_bind_every_scope_field_revision_and_exact_text() {
    let scope = question_scope();
    let mut policy = StandingQuestionPolicy::new(id("standing-1"), scope.clone());
    assert_eq!(
        policy.authorize(&scope, "Question?".into(), &id("standing-1")),
        Err(QuestionDenial::Disabled)
    );
    policy.set_enabled(true);
    let text = "  Which release is compatible?\n";
    let authorized = policy
        .authorize(&scope, text.into(), &id("standing-1"))
        .expect("scope matches");
    assert_eq!(authorized.exact_question(), text);
    assert_eq!(authorized.policy_revision(), &id("standing-1"));
    for field in 0..6 {
        let mut changed = scope.clone();
        let target = match field {
            0 => &mut changed.requester,
            1 => &mut changed.responder,
            2 => &mut changed.collection,
            3 => &mut changed.purpose,
            4 => &mut changed.onward_destination,
            _ => &mut changed.model,
        };
        *target = id("unapproved");
        assert_eq!(
            policy.authorize(&changed, text.into(), &id("standing-1")),
            Err(QuestionDenial::ScopeMismatch)
        );
    }
    assert_eq!(
        policy.authorize(&scope, text.into(), &id("standing-0")),
        Err(QuestionDenial::StaleRevision)
    );
    assert_eq!(
        policy.authorize(&scope, " ".into(), &id("standing-1")),
        Err(QuestionDenial::InvalidQuestion)
    );
}

fn release() -> ReleaseManifest {
    ReleaseManifest {
        version: id("2.1.1"),
        source_revision: id("source-revision"),
        manifest_digest: id("manifest-sha256"),
        channel: id("maintenance"),
        artifacts: vec![ReleaseArtifact {
            architecture: Architecture::Amd64,
            digest: id("amd64-sha256"),
        }],
        compatible_from_versions: vec![id("2.1.0")],
        minimum_schema: 42,
        maximum_schema: 42,
        maintenance: true,
        has_migrations: false,
        changes_trust: false,
        changes_permissions: false,
    }
}

fn upgrade_context() -> UpgradeContext {
    UpgradeContext {
        area: Area::Homelab,
        now: UnixSeconds(10_000),
        london_time: LondonLocalTime::new(2, 30).expect("valid time"),
        architecture: Architecture::Amd64,
        installed_version: id("2.1.0"),
        schema: 42,
        enabled_channel: Some(id("maintenance")),
        signature: LocalSignature::Verified {
            manifest_digest: id("manifest-sha256"),
            authority: id("local-release-key"),
        },
        backup: Some(BackupEvidence {
            id: id("backup"),
            area: Area::Homelab,
            completed_at: UnixSeconds(9_000),
            schema: 42,
            restore_verified: true,
        }),
        maximum_backup_age_seconds: 3_600,
        conflicting_work: false,
    }
}

#[test]
fn maintenance_window_is_london_two_inclusive_to_three_exclusive() {
    for (hour, minute, eligible) in [(1, 59, false), (2, 0, true), (2, 59, true), (3, 0, false)] {
        let mut context = upgrade_context();
        context.london_time = LondonLocalTime::new(hour, minute).expect("valid local time");
        assert_eq!(evaluate_upgrade(&release(), &context).eligible(), eligible);
    }
    assert!(LondonLocalTime::new(24, 0).is_err());
    assert!(LondonLocalTime::new(2, 60).is_err());
}

#[test]
fn automatic_upgrade_checks_each_local_safety_gate() {
    let release = release();
    assert!(evaluate_upgrade(&release, &upgrade_context()).eligible());
    type ContextMutation = Box<dyn Fn(&mut UpgradeContext)>;
    let cases: Vec<(UpgradeBlocker, ContextMutation)> = vec![
        (
            UpgradeBlocker::AreaRequiresHumanApproval,
            Box::new(|c| c.area = Area::Trust),
        ),
        (
            UpgradeBlocker::ChannelNotEnabled,
            Box::new(|c| c.enabled_channel = None),
        ),
        (
            UpgradeBlocker::SignatureNotVerified,
            Box::new(|c| c.signature = LocalSignature::Invalid),
        ),
        (
            UpgradeBlocker::ManifestDigestMismatch,
            Box::new(|c| {
                c.signature = LocalSignature::Verified {
                    manifest_digest: id("different"),
                    authority: id("key"),
                }
            }),
        ),
        (
            UpgradeBlocker::ArchitectureUnavailable,
            Box::new(|c| c.architecture = Architecture::Arm64),
        ),
        (
            UpgradeBlocker::IncompatibleVersion,
            Box::new(|c| c.installed_version = id("1.0.0")),
        ),
        (
            UpgradeBlocker::IncompatibleSchema,
            Box::new(|c| c.schema = 43),
        ),
        (UpgradeBlocker::BackupMissing, Box::new(|c| c.backup = None)),
        (
            UpgradeBlocker::BackupNotFresh,
            Box::new(|c| c.maximum_backup_age_seconds = 999),
        ),
        (
            UpgradeBlocker::ConflictingWork,
            Box::new(|c| c.conflicting_work = true),
        ),
    ];
    for (expected, change) in cases {
        let mut context = upgrade_context();
        change(&mut context);
        let decision = evaluate_upgrade(&release, &context);
        assert!(!decision.eligible());
        assert!(
            decision.blockers().contains(&expected),
            "missing {expected:?}"
        );
    }
}

#[test]
fn release_claims_cannot_authorize_migrations_or_security_changes() {
    let context = upgrade_context();
    let mut release = release();
    release.has_migrations = true;
    release.changes_trust = true;
    release.changes_permissions = true;
    release.maintenance = false;
    release.artifacts.push(release.artifacts[0].clone());
    let decision = evaluate_upgrade(&release, &context);
    for blocker in [
        UpgradeBlocker::MigrationRequiresHumanApproval,
        UpgradeBlocker::TrustChangeRequiresHumanApproval,
        UpgradeBlocker::PermissionChangeRequiresHumanApproval,
        UpgradeBlocker::NotMaintenance,
        UpgradeBlocker::AmbiguousArchitecture,
    ] {
        assert!(decision.blockers().contains(&blocker));
    }
}

#[test]
fn backup_must_be_recoverable_current_area_schema_and_not_from_future() {
    for case in 0..4 {
        let mut context = upgrade_context();
        let backup = context.backup.as_mut().expect("backup");
        let blocker = match case {
            0 => {
                backup.area = Area::Workshop;
                UpgradeBlocker::BackupWrongArea
            }
            1 => {
                backup.schema = 41;
                UpgradeBlocker::BackupWrongSchema
            }
            2 => {
                backup.restore_verified = false;
                UpgradeBlocker::BackupNotRecoverable
            }
            _ => {
                backup.completed_at = UnixSeconds(10_001);
                UpgradeBlocker::BackupNotFresh
            }
        };
        assert!(
            evaluate_upgrade(&release(), &context)
                .blockers()
                .contains(&blocker)
        );
    }
}

#[test]
fn stable_ids_validate_json_and_contracts_expose_uncertainty() {
    assert!(serde_json::from_str::<StableId>("\"\"").is_err());
    assert!(serde_json::from_str::<StableId>(r#"" ""#).is_err());
    assert!(serde_json::from_str::<StableId>("\"bad\\nvalue\"").is_err());
    assert_eq!(
        serde_json::to_value(AttemptState::Uncertain).expect("serialize"),
        serde_json::json!({"status": "uncertain"})
    );
    let request = request("a", 1);
    let roundtrip: ProviderRequest =
        serde_json::from_value(serde_json::to_value(&request).expect("serialize")).expect("parse");
    assert_eq!(roundtrip, request);
    let mut unknown = serde_json::to_value(&request).expect("serialize");
    unknown["override_model"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ProviderRequest>(unknown).is_err());
}
