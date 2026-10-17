// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use codifix_federation::{expertise::*, publication::*, *};
use uuid::Uuid;

fn instance(n: u128) -> InstanceId {
    InstanceId::new(Uuid::from_u128(n)).unwrap()
}
fn record(n: u128) -> RecordId {
    RecordId::new(Uuid::from_u128(n)).unwrap()
}
fn space() -> SpaceId {
    SpaceId::new(Uuid::from_u128(100)).unwrap()
}
fn time() -> chrono::DateTime<chrono::Utc> {
    "2026-09-10T00:00:00Z".parse().unwrap()
}
fn delivery(seq: u32, publisher: u128, record: PublicationRecord) -> PublicationDelivery {
    PublicationDelivery::new(
        AuthoritySequence::new(seq).unwrap(),
        RecordLocator::new(format!("record:{seq}")).unwrap(),
        PublicationEnvelope {
            schema_version: 3,
            space: space(),
            publisher: instance(publisher),
            record_id: self::record(u128::from(seq)),
            author: AuthorIdentity::new("expert").unwrap(),
            authored_at: time(),
            causation: None,
            record,
        },
    )
    .unwrap()
}
fn ad() -> CapabilityAdvertisement {
    CapabilityAdvertisement {
        owner: instance(2),
        collection: record(99),
        revision: StateRevision::new(1).unwrap(),
        collection_digest: CanonicalJsonCodec::digest(&"snapshot").unwrap(),
        name: "Homelab operations".into(),
        description: "Approved operational knowledge".into(),
        topics: vec!["homelab".into()],
        modes: vec![AnswerMode::Deterministic],
        requesters: vec![instance(3)],
        model_destinations: vec![],
        output_destinations: vec!["local_human".into()],
        active: true,
        limits: ExpertiseLimits::default(),
    }
}
fn question() -> ExpertiseQuestion {
    let ad = ad();
    ExpertiseQuestion {
        request_id: Uuid::from_u128(88),
        space: space(),
        requester: instance(3),
        principal: Uuid::from_u128(10),
        capability: ad.key(),
        revision: ad.revision,
        collection_digest: ad.collection_digest,
        question: "What is the restore procedure?".into(),
        mode: AnswerMode::Deterministic,
        model_destination: None,
        output_destination: "local_human".into(),
        created_at: time(),
        expires_at: time() + chrono::Duration::seconds(120),
    }
}
fn ready() -> PublicationState {
    let mut state = PublicationState::new(space(), instance(1));
    for (member, capability) in [
        (2, SpaceCapability::Pull),
        (2, SpaceCapability::Publish),
        (2, SpaceCapability::AnswerExpertise),
        (3, SpaceCapability::Pull),
        (3, SpaceCapability::AskExpertise),
    ] {
        let d = delivery(
            state.sequence() + 1,
            1,
            PublicationRecord::Membership {
                member: instance(member),
                capability,
            },
        );
        assert_eq!(state.apply(instance(1), d), PublicationOutcome::Applied);
    }
    let d = delivery(state.sequence() + 1, 2, PublicationRecord::Expertise(ad()));
    assert_eq!(state.apply(instance(2), d), PublicationOutcome::Applied);
    state
}
#[test]
fn knowledge_membership_does_not_confer_expertise_and_new_semantics_require_v3() {
    let mut state = PublicationState::new(space(), instance(1));
    assert!(!state.has_capability(instance(1), SpaceCapability::AskExpertise));
    for capability in [
        SpaceCapability::AskExpertise,
        SpaceCapability::AnswerExpertise,
    ] {
        let mut d = delivery(
            1,
            1,
            PublicationRecord::Membership {
                member: instance(2),
                capability,
            },
        );
        d.envelope.schema_version = 2;
        d.digest = CanonicalJsonCodec::digest(&d.envelope).unwrap();
        assert_eq!(
            state.apply(instance(1), d),
            PublicationOutcome::Rejected(PublicationReject::UnsupportedSchema)
        );
    }
    let mut d = delivery(1, 2, PublicationRecord::Expertise(ad()));
    d.envelope.schema_version = 2;
    d.digest = CanonicalJsonCodec::digest(&d.envelope).unwrap();
    assert_eq!(
        state.apply(instance(2), d),
        PublicationOutcome::Rejected(PublicationReject::UnsupportedSchema)
    );
    assert_eq!(state.sequence(), 0);
}
#[test]
fn exact_query_authorization_binds_scope_recipient_revision_destination_and_time() {
    let state = ready();
    let q = question();
    assert!(state.authorize_expertise(instance(3), &q, time()).is_ok());
    let mut changed = q.clone();
    changed.requester = instance(4);
    assert_eq!(
        state.authorize_expertise(instance(4), &changed, time()),
        Err(ExpertiseReject::Unauthorized)
    );
    assert_eq!(
        state.authorize_expertise(instance(4), &q, time()),
        Err(ExpertiseReject::Unauthorized)
    );
    changed = q.clone();
    changed.revision = StateRevision::new(2).unwrap();
    assert_eq!(
        state.authorize_expertise(instance(3), &changed, time()),
        Err(ExpertiseReject::StaleCollection)
    );
    changed = q.clone();
    changed.output_destination = "cloud_model".into();
    assert_eq!(
        state.authorize_expertise(instance(3), &changed, time()),
        Err(ExpertiseReject::DestinationDenied)
    );
    assert_eq!(
        state.authorize_expertise(instance(3), &q, q.expires_at),
        Err(ExpertiseReject::Expired)
    );
    changed = q.clone();
    changed.space = SpaceId::new(Uuid::from_u128(101)).unwrap();
    assert_eq!(
        state.authorize_expertise(instance(3), &changed, time()),
        Err(ExpertiseReject::Unauthorized)
    );
    changed = q;
    changed.model_destination = Some("cloud_model".into());
    assert!(changed.validate().is_err());
}
#[test]
fn revoke_and_withdraw_block_new_questions_without_erasing_old_advertisements() {
    for (member, capability) in [
        (2, SpaceCapability::AnswerExpertise),
        (3, SpaceCapability::AskExpertise),
    ] {
        let mut state = ready();
        let d = delivery(
            7,
            1,
            PublicationRecord::Revocation {
                member: instance(member),
                capability: Some(capability),
            },
        );
        assert_eq!(state.apply(instance(1), d), PublicationOutcome::Applied);
        assert_eq!(
            state.authorize_expertise(instance(3), &question(), time()),
            Err(ExpertiseReject::Unauthorized)
        );
        assert!(state.expertise(ad().key()).is_some());
    }
    let mut state = ready();
    let mut inactive = ad();
    inactive.active = false;
    inactive.revision = StateRevision::new(2).unwrap();
    let d = delivery(7, 2, PublicationRecord::Expertise(inactive));
    assert_eq!(state.apply(instance(2), d), PublicationOutcome::Applied);
    assert_eq!(
        state.authorize_expertise(instance(3), &question(), time()),
        Err(ExpertiseReject::Unauthorized)
    );
    assert_eq!(state.accepted().len(), 7);
}
#[test]
fn advertisements_are_owned_monotonic_idempotent_and_fail_without_projection_changes() {
    let mut state = ready();
    let last = state.accepted().last().unwrap().clone();
    assert_eq!(
        state.apply(instance(2), last.clone()),
        PublicationOutcome::Duplicate
    );
    let prior = state.clone();
    let mut altered = last;
    altered.envelope.author = AuthorIdentity::new("forged").unwrap();
    altered.digest = CanonicalJsonCodec::digest(&altered.envelope).unwrap();
    assert_eq!(
        state.apply(instance(2), altered),
        PublicationOutcome::Rejected(PublicationReject::Equivocation)
    );
    let d = delivery(7, 1, PublicationRecord::Expertise(ad()));
    assert_eq!(
        state.apply(instance(1), d),
        PublicationOutcome::Rejected(PublicationReject::NotOrigin)
    );
    let mut gap = ad();
    gap.revision = StateRevision::new(3).unwrap();
    assert_eq!(
        state.apply(
            instance(2),
            delivery(7, 2, PublicationRecord::Expertise(gap))
        ),
        PublicationOutcome::Quarantined(PublicationQuarantine::RevisionGap)
    );
    assert_eq!(state, prior);
}
#[test]
fn limits_and_model_destination_names_are_bounded() {
    for invalid in ["", "https://model.example", "../secrets", "with space"] {
        let mut a = ad();
        a.output_destinations = vec![invalid.into()];
        assert!(a.validate().is_err());
    }
    let mut a = ad();
    a.modes.push(AnswerMode::Model);
    assert!(a.validate().is_err());
    a.model_destinations.push("approved_model".into());
    assert!(a.validate().is_ok());
    a.limits.context_bytes += 1;
    assert!(a.validate().is_err());
    let mut q = question();
    q.question = "x".repeat(MAX_QUESTION_BYTES + 1);
    assert!(q.validate().is_err());
}
proptest::proptest! {
    #[test]
    fn arbitrarily_repeated_advertisements_never_change_the_projection(repeats in 0usize..100) {
        let mut state = ready(); let original = state.clone();
        let last = state.accepted().last().unwrap().clone();
        for _ in 0..repeats { proptest::prop_assert_eq!(state.apply(instance(2), last.clone()), PublicationOutcome::Duplicate); }
        proptest::prop_assert_eq!(state, original);
    }
}
