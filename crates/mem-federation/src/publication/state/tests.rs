// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use proptest::prelude::*;

fn instance(id: u128) -> InstanceId {
    InstanceId::new(Uuid::from_u128(id)).unwrap()
}
fn id(value: u128) -> RecordId {
    RecordId::new(Uuid::from_u128(value)).unwrap()
}
fn space() -> SpaceId {
    SpaceId::new(Uuid::from_u128(100)).unwrap()
}
fn stream(origin: u128) -> StreamId {
    StreamId {
        origin: instance(origin),
        canonical: id(10),
    }
}
fn now() -> DateTime<Utc> {
    "2026-09-08T00:00:00Z".parse().unwrap()
}
fn author() -> AuthorIdentity {
    AuthorIdentity::new("original-expert").unwrap()
}
fn content(text: &str) -> MemoryContent {
    MemoryContent {
        text: text.into(),
        summary: text.into(),
        memory_type: MemoryType::Decision,
        confidence: ConfidencePpm::from_confidence(0.8).unwrap(),
        importance: 3,
        tags: vec![],
    }
}
fn source(local: u32) -> SourceVersionRef {
    SourceVersionRef {
        record_id: id(999),
        version: SourceVersion::new(local).unwrap(),
    }
}
fn version(origin: u128, shared: u32, local: u32) -> PublicationRecord {
    PublicationRecord::MemoryVersion {
        stream: stream(origin),
        publication: MemoryPublication {
            version: SharedVersion::new(shared).unwrap(),
            source: source(local),
            content: content("knowledge"),
            evidence: vec![],
        },
    }
}
fn state_record(revision: u32, version: u32, tombstone: bool) -> PublicationRecord {
    PublicationRecord::CanonicalState {
        stream: stream(2),
        state: StatePublication {
            revision: StateRevision::new(revision).unwrap(),
            content_version: SharedVersion::new(version).unwrap(),
            confidence: ConfidencePpm::from_confidence(0.8).unwrap(),
            status: MemoryStatus::Active,
            tombstone,
        },
    }
}
fn delivery(
    sequence: u32,
    publisher: u128,
    record_id: u128,
    record: PublicationRecord,
) -> PublicationDelivery {
    PublicationDelivery::new(
        AuthoritySequence::new(sequence).unwrap(),
        RecordLocator::new(format!("record:{record_id}")).unwrap(),
        PublicationEnvelope {
            schema_version: PUBLICATION_SCHEMA_VERSION,
            space: space(),
            publisher: instance(publisher),
            record_id: id(record_id),
            author: author(),
            authored_at: now(),
            causation: None,
            record,
        },
    )
    .unwrap()
}
fn accept(
    state: &mut PublicationState,
    publisher: u128,
    record_id: u128,
    record: PublicationRecord,
) -> PublicationDelivery {
    let delivery = delivery(state.sequence() + 1, publisher, record_id, record);
    let encoded = serde_json::to_value(&delivery).unwrap();
    let decoded: PublicationDelivery = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, delivery);
    assert_eq!(
        state.apply(instance(publisher), delivery.clone()),
        PublicationOutcome::Applied
    );
    delivery
}
fn initial() -> PublicationState {
    let mut state = PublicationState::new(space(), instance(1));
    for member in [2, 3] {
        accept(
            &mut state,
            1,
            member,
            PublicationRecord::Membership {
                member: instance(member),
                capability: SpaceCapability::Publish,
            },
        );
    }
    state
}
fn reject_unchanged(
    state: &mut PublicationState,
    publisher: u128,
    record: PublicationDelivery,
    expected: PublicationOutcome,
) {
    let before = state.clone();
    assert_eq!(state.apply(instance(publisher), record), expected);
    assert_eq!(*state, before);
}
fn reviewed(origin: u128, text: &str) -> ReviewedEvidence {
    let snapshot = EvidenceSnapshot {
        space: space(),
        stream: stream(origin),
        public_source_label: "approved incident extract".into(),
        selected_text: text.into(),
        observed_at: now(),
    };
    let review = ReviewReceipt {
        snapshot_digest: CanonicalJsonCodec::digest(&snapshot).unwrap(),
        reviewer: author(),
        reviewed_at: now(),
    };
    ReviewedEvidence::new(snapshot, review).unwrap()
}

#[test]
fn validated_ids_counters_and_confidence_fail_closed_on_deserialization() {
    assert!(InstanceId::new(Uuid::nil()).is_err());
    assert!(SpaceId::new(Uuid::nil()).is_err());
    assert!(RecordId::new(Uuid::nil()).is_err());
    assert!(
        serde_json::from_str::<InstanceId>("\"00000000-0000-0000-0000-000000000000\"").is_err()
    );
    assert!(serde_json::from_str::<SharedVersion>("0").is_err());
    assert!(serde_json::from_str::<StateRevision>("-1").is_err());
    assert!(serde_json::from_str::<ConfidencePpm>("1000001").is_err());
    for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert!(ConfidencePpm::from_confidence(value).is_err());
    }
    assert_eq!(
        check_counter(u32::MAX, 1, PublicationQuarantine::VersionGap),
        reject(PublicationReject::CounterExhausted)
    );
}

#[test]
fn shared_versions_and_state_revisions_are_independent_and_origin_scoped() {
    let mut state = initial();
    accept(&mut state, 2, 10, version(2, 1, 27));
    accept(&mut state, 2, 11, state_record(1, 1, false));
    accept(&mut state, 2, 12, state_record(2, 1, true));
    assert_eq!(state.history(stream(2)).len(), 1);
    assert!(state.current_state(stream(2)).unwrap().tombstone);
    accept(&mut state, 2, 13, version(2, 2, 42));
    assert!(state.current_state(stream(2)).is_none());
    accept(&mut state, 2, 14, state_record(3, 2, false));
    accept(&mut state, 3, 10, version(3, 1, 99)); // same IDs/locator at another origin
    assert_eq!(state.state_revision(stream(2)), 3);
    assert_eq!(state.content_version(stream(2)), 2);
    assert_eq!(state.content_version(stream(3)), 1);
    assert_eq!(
        state.history(stream(2))[1].publication.source.version.get(),
        42
    );
    assert_eq!(state.history(stream(2))[1].author, author());
    assert_eq!(state.history(stream(2))[1].key.publisher, instance(2));
    let seq = state.sequence() + 1;
    reject_unchanged(
        &mut state,
        3,
        delivery(seq, 3, 15, version(2, 3, 45)),
        PublicationOutcome::Rejected(PublicationReject::NotOrigin),
    );
    reject_unchanged(
        &mut state,
        2,
        delivery(seq, 2, 16, state_record(4, 1, true)),
        PublicationOutcome::Rejected(PublicationReject::CounterConflict),
    );
    reject_unchanged(
        &mut state,
        2,
        delivery(seq, 2, 17, state_record(5, 2, true)),
        PublicationOutcome::Quarantined(PublicationQuarantine::RevisionGap),
    );
}

#[test]
fn reviewed_evidence_binds_bytes_context_and_does_not_leak_local_source_objects() {
    let original = reviewed(2, "The timeout is 30 seconds.\n");
    assert_eq!(
        ContentDigest::sha256_bytes(&original.published_snapshot_bytes().unwrap()),
        original.review().snapshot_digest
    );
    let json = serde_json::to_value(&original).unwrap();
    for (field, value) in [
        (
            "selected_text",
            serde_json::json!("The timeout is 90 seconds.\n"),
        ),
        ("space", serde_json::json!(Uuid::from_u128(200))),
        (
            "public_source_label",
            serde_json::json!("different evidence"),
        ),
    ] {
        let mut changed = json.clone();
        changed["snapshot"][field] = value;
        assert!(serde_json::from_value::<ReviewedEvidence>(changed).is_err());
    }
    let mut extra = json.clone();
    extra["snapshot"]["file_path"] = serde_json::json!("/private/capture");
    assert!(serde_json::from_value::<ReviewedEvidence>(extra).is_err());
    assert_eq!(
        serde_json::from_value::<ReviewedEvidence>(json).unwrap(),
        original
    );
    let mut state = initial();
    let evidence = accept(&mut state, 2, 20, PublicationRecord::Evidence(original));
    let mut memory = version(2, 1, 27);
    if let PublicationRecord::MemoryVersion { publication, .. } = &mut memory {
        publication.evidence.push(evidence.envelope.key());
    }
    accept(&mut state, 2, 21, memory.clone());
    if let PublicationRecord::MemoryVersion { stream: target, .. } = &mut memory {
        *target = stream(3);
    }
    let seq = state.sequence() + 1;
    reject_unchanged(
        &mut state,
        3,
        delivery(seq, 3, 22, memory),
        PublicationOutcome::Rejected(PublicationReject::EvidenceScope),
    );
    let changed = delivery(
        evidence.sequence.get(),
        2,
        20,
        PublicationRecord::Evidence(reviewed(2, "revised snapshot")),
    );
    reject_unchanged(
        &mut state,
        2,
        changed,
        PublicationOutcome::Rejected(PublicationReject::Equivocation),
    );
}

#[test]
fn proposal_approval_is_origin_owned_atomic_and_preserves_correction_authorship() {
    let mut state = initial();
    accept(&mut state, 2, 30, version(2, 1, 27));
    let proposal = accept(
        &mut state,
        3,
        31,
        PublicationRecord::Proposal(CorrectionProposal {
            target: stream(2),
            base_version: SharedVersion::new(1).unwrap(),
            content: content("corrected"),
            evidence: vec![],
        }),
    );
    assert_eq!(state.content_version(stream(2)), 1);
    let mut approval = delivery(
        state.sequence() + 1,
        3,
        32,
        PublicationRecord::ResolveProposal {
            proposal: proposal.envelope.key(),
            source: Some(source(42)),
        },
    );
    approval.envelope.causation = Some(proposal.envelope.key());
    approval.digest = CanonicalJsonCodec::digest(&approval.envelope).unwrap();
    reject_unchanged(
        &mut state,
        3,
        approval.clone(),
        PublicationOutcome::Rejected(PublicationReject::NotOrigin),
    );
    approval.envelope.publisher = instance(2);
    approval.envelope.author = AuthorIdentity::new("approver").unwrap();
    approval.digest = CanonicalJsonCodec::digest(&approval.envelope).unwrap();
    assert_eq!(
        state.apply(instance(2), approval.clone()),
        PublicationOutcome::Applied
    );
    let version = &state.history(stream(2))[1];
    assert_eq!(version.publication.content.text, "corrected");
    assert_eq!(version.publication.source.version.get(), 42);
    assert_eq!(version.author, author()); // not the approver or publisher
    assert_eq!(version.key.publisher, instance(2));
    assert_eq!(state.accepted_record(version.key).unwrap(), &approval);
    assert!(matches!(
        state.proposal(proposal.envelope.key()).unwrap().resolution,
        ProposalResolution::Accepted { .. }
    ));
    assert_eq!(
        state.apply(instance(2), approval),
        PublicationOutcome::Duplicate
    );
}

#[test]
fn stale_proposals_can_be_rejected_but_cannot_overwrite_new_content() {
    let mut state = initial();
    accept(&mut state, 2, 40, version(2, 1, 27));
    let proposal = accept(
        &mut state,
        3,
        41,
        PublicationRecord::Proposal(CorrectionProposal {
            target: stream(2),
            base_version: SharedVersion::new(1).unwrap(),
            content: content("stale correction"),
            evidence: vec![],
        }),
    );
    accept(&mut state, 2, 42, version(2, 2, 31));
    let mut decision = delivery(
        state.sequence() + 1,
        2,
        43,
        PublicationRecord::ResolveProposal {
            proposal: proposal.envelope.key(),
            source: Some(source(42)),
        },
    );
    decision.envelope.causation = Some(proposal.envelope.key());
    decision.digest = CanonicalJsonCodec::digest(&decision.envelope).unwrap();
    reject_unchanged(
        &mut state,
        2,
        decision.clone(),
        PublicationOutcome::Rejected(PublicationReject::StaleProposal),
    );
    decision.envelope.record = PublicationRecord::ResolveProposal {
        proposal: proposal.envelope.key(),
        source: None,
    };
    decision.digest = CanonicalJsonCodec::digest(&decision.envelope).unwrap();
    assert_eq!(
        state.apply(instance(2), decision),
        PublicationOutcome::Applied
    );
    assert_eq!(state.content_version(stream(2)), 2);
}

#[test]
fn relation_retraction_keeps_history_and_cannot_change_endpoints() {
    let mut state = initial();
    accept(&mut state, 2, 50, version(2, 1, 27));
    accept(&mut state, 3, 51, version(3, 1, 9));
    let mut relation = RelationPublication {
        id: id(52),
        source: stream(2),
        target: stream(3),
        relation_type: MemoryRelationType::Supports,
        revision: StateRevision::new(1).unwrap(),
        active: true,
    };
    accept(
        &mut state,
        2,
        53,
        PublicationRecord::Relation(relation.clone()),
    );
    relation.revision = StateRevision::new(2).unwrap();
    relation.active = false;
    accept(
        &mut state,
        2,
        54,
        PublicationRecord::Relation(relation.clone()),
    );
    let key = RecordKey {
        publisher: instance(2),
        id: id(52),
    };
    assert_eq!(state.relation_history(key).len(), 2);
    assert!(!state.relation_history(key)[1].active);
    relation.revision = StateRevision::new(3).unwrap();
    relation.target = stream(2);
    let seq = state.sequence() + 1;
    reject_unchanged(
        &mut state,
        2,
        delivery(seq, 2, 55, PublicationRecord::Relation(relation)),
        PublicationOutcome::Rejected(PublicationReject::RelationConflict),
    );
}

#[test]
fn delivery_failures_never_mutate_state_and_revocation_preserves_prior_records() {
    let mut state = initial();
    let seq = state.sequence() + 1;
    let record = delivery(seq, 2, 60, version(2, 1, 27));
    reject_unchanged(
        &mut state,
        3,
        record.clone(),
        PublicationOutcome::Rejected(PublicationReject::ForgedPublisher),
    );
    let mut wrong = record.clone();
    wrong.envelope.space = SpaceId::new(Uuid::from_u128(200)).unwrap();
    reject_unchanged(
        &mut state,
        2,
        wrong,
        PublicationOutcome::Rejected(PublicationReject::WrongSpace),
    );
    let mut wrong = record.clone();
    wrong.envelope.schema_version = 999;
    reject_unchanged(
        &mut state,
        2,
        wrong,
        PublicationOutcome::Rejected(PublicationReject::UnsupportedSchema),
    );
    let mut wrong = record.clone();
    wrong.envelope.author = AuthorIdentity::new("forged").unwrap();
    reject_unchanged(
        &mut state,
        2,
        wrong,
        PublicationOutcome::Rejected(PublicationReject::IntegrityMismatch),
    );
    let mut wrong = record.clone();
    wrong.sequence = AuthoritySequence::new(seq + 1).unwrap();
    reject_unchanged(
        &mut state,
        2,
        wrong,
        PublicationOutcome::Quarantined(PublicationQuarantine::OrderGap),
    );
    assert_eq!(
        state.apply(instance(2), record.clone()),
        PublicationOutcome::Applied
    );
    let mut wrong = record.clone();
    wrong.sequence = AuthoritySequence::new(seq + 1).unwrap();
    reject_unchanged(
        &mut state,
        2,
        wrong,
        PublicationOutcome::Rejected(PublicationReject::Equivocation),
    );
    accept(
        &mut state,
        1,
        61,
        PublicationRecord::Revocation {
            member: instance(2),
            capability: None,
        },
    );
    let seq = state.sequence() + 1;
    reject_unchanged(
        &mut state,
        2,
        delivery(seq, 2, 62, version(2, 2, 31)),
        PublicationOutcome::Rejected(PublicationReject::Unauthorized),
    );
    assert_eq!(
        state.apply(instance(2), record),
        PublicationOutcome::Duplicate
    );
    assert_eq!(state.history(stream(2)).len(), 1);
    let mut replayed = PublicationState::new(space(), instance(1));
    replayed.replay(
        state
            .accepted()
            .iter()
            .cloned()
            .map(|r| (r.envelope.publisher, r)),
    );
    assert_eq!(replayed, state);
}

proptest! {
    #[test]
    fn arbitrary_local_versions_do_not_control_shared_order(locals in prop::collection::vec(1u32..u32::MAX, 1..30)) {
        let mut state = initial();
        for (index, local) in locals.iter().enumerate() {
            let record = accept(&mut state, 2, 1000 + index as u128, version(2, index as u32 + 1, *local));
            let before = state.clone();
            prop_assert_eq!(state.apply(instance(2), record), PublicationOutcome::Duplicate);
            prop_assert_eq!(&state, &before);
        }
        prop_assert_eq!(state.content_version(stream(2)), locals.len() as u32);
    }
    #[test]
    fn ppm_round_trip_has_at_most_one_ppm_error(value in 0.0f32..=1.0) {
        let ppm = ConfidencePpm::from_confidence(value).unwrap();
        prop_assert!((value - ppm.as_confidence()).abs() <= 0.000001);
    }
}
