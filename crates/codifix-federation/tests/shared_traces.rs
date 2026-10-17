// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fs, path::PathBuf};

use chrono::{TimeZone, Utc};
use codifix_federation::{
    ApplyOutcome, AuthenticatedRecord, AuthorIdentity, FederationRecord, MembershipRecord,
    MemoryVersionRecord, ProjectSpaceId, QuarantineReason, RecordLocator, RejectReason,
    ReplicaState, RevocationRecord, SharedRecordEnvelope, SourceIdentity, SpaceCapability,
    TransportRecord,
};
use codifix_record::{MemoryStatus, MemoryType};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TraceFixture {
    name: String,
    records: Vec<TraceRecord>,
    expected: TraceExpected,
}

#[derive(Debug, Deserialize)]
struct TraceExpected {
    outcomes: Vec<String>,
    summary: TraceSummary,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TraceSummary {
    seen_request_count: usize,
    evidence_count: usize,
    memory_version_count: usize,
    active_canonical_count: usize,
    active_publishers: Vec<String>,
    revoked_publishers: Vec<String>,
    canonical_heads: Vec<TraceCanonicalHead>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TraceCanonicalHead {
    canonical_id: String,
    version: u32,
    publisher: String,
    author: String,
    source: String,
    locator: String,
    digest: String,
    evidence_count: usize,
    tombstone: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TraceRecord {
    request_id: String,
    authority: String,
    publisher: String,
    kind: String,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    capability: Option<String>,
    #[serde(default)]
    evidence_id: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    locator: Option<String>,
    #[serde(default)]
    digest: Option<String>,
    #[serde(default)]
    canonical_id: Option<String>,
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    evidence_refs: Vec<String>,
    #[serde(default)]
    tombstone: bool,
}

#[test]
fn shared_federation_traces_match_expected_outcomes_and_summaries() {
    let fixture_dir = trace_dir();
    let mut paths = fs::read_dir(&fixture_dir)
        .expect("read trace dir")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(
        paths.len(),
        9,
        "expected exactly nine shared trace fixtures"
    );

    let mut digest_mismatches = Vec::new();
    let mut outcome_mismatches = Vec::new();
    let mut summary_mismatches = Vec::new();
    for path in paths {
        let fixture = load_fixture(&path);
        let (outcomes, summary, digests) = execute_fixture(&fixture);

        for mismatch in digests {
            digest_mismatches.push(format!("{}: {}", fixture.name, mismatch));
        }

        if outcomes != fixture.expected.outcomes {
            outcome_mismatches.push(format!(
                "{} outcomes expected {:?} actual {:?}",
                fixture.name, fixture.expected.outcomes, outcomes
            ));
        }
        if summary != fixture.expected.summary {
            summary_mismatches.push(format!(
                "{} summary expected {:?} actual {:?}",
                fixture.name, fixture.expected.summary, summary
            ));
        }
    }

    let mut failures = Vec::new();
    if !outcome_mismatches.is_empty() {
        failures.push(format!(
            "outcome mismatches:\n{}",
            outcome_mismatches.join("\n")
        ));
    }
    if !summary_mismatches.is_empty() {
        failures.push(format!(
            "summary mismatches:\n{}",
            summary_mismatches.join("\n")
        ));
    }
    if !digest_mismatches.is_empty() {
        failures.push(format!(
            "digest mismatches:\n{}",
            digest_mismatches.join("\n")
        ));
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

fn execute_fixture(fixture: &TraceFixture) -> (Vec<String>, TraceSummary, Vec<String>) {
    let space_id = ProjectSpaceId::new("trace-space").expect("space");
    let authority = AuthorIdentity::new("root").expect("authority");
    let mut replica = ReplicaState::new(space_id.clone(), authority);
    let mut outcomes = Vec::with_capacity(fixture.records.len());
    let mut accepted_trace_records = 0usize;
    let mut digest_mismatches = Vec::new();

    let authored_at = Utc
        .with_ymd_and_hms(2026, 9, 4, 12, 0, 0)
        .single()
        .expect("fixed base timestamp");
    for record in &fixture.records {
        let publisher =
            author_identity(&record.publisher).expect("authenticated fixture publisher");
        match record.kind.as_str() {
            "grant" => {
                let transport = membership_transport(&space_id, record, authored_at, true);
                let outcome = replica.apply_authenticated(AuthenticatedRecord::new(
                    publisher,
                    transport.expect("valid grant transport"),
                ));
                if outcome == ApplyOutcome::Applied {
                    accepted_trace_records += 1;
                }
                outcomes.push(map_simple_outcome(outcome, "acceptedGrant"));
            }
            "revoke" => {
                let transport = membership_transport(&space_id, record, authored_at, false);
                let outcome = replica.apply_authenticated(AuthenticatedRecord::new(
                    publisher,
                    transport.expect("valid revoke transport"),
                ));
                if outcome == ApplyOutcome::Applied {
                    accepted_trace_records += 1;
                }
                outcomes.push(map_simple_outcome(outcome, "acceptedRevoke"));
            }
            "evidence" => {
                let transport = evidence_transport(&space_id, record, authored_at);
                let expected_digest = record.digest.as_deref().expect("fixture digest");
                let transport = transport.expect("valid evidence transport");
                let actual_digest = format!("sha256:{}", transport.content_digest().as_hex());
                if actual_digest != expected_digest {
                    digest_mismatches.push(format!(
                        "record {} digest expected {} actual {}",
                        record.request_id, expected_digest, actual_digest
                    ));
                }
                let outcome =
                    replica.apply_authenticated(AuthenticatedRecord::new(publisher, transport));
                if outcome == ApplyOutcome::Applied {
                    accepted_trace_records += 1;
                }
                outcomes.push(map_simple_outcome(outcome, "acceptedEvidence"));
            }
            "memory" => {
                let memory =
                    memory_transport(&space_id, record, authored_at).expect("valid memory");
                let actual_digest = format!("sha256:{}", memory.content_digest().as_hex());
                let expected_digest = record.digest.as_deref().expect("fixture digest");
                if actual_digest != expected_digest {
                    digest_mismatches.push(format!(
                        "record {} digest expected {} actual {}",
                        record.request_id, expected_digest, actual_digest
                    ));
                }
                let outcome = match replica
                    .apply_authenticated(AuthenticatedRecord::new(publisher, memory))
                {
                    ApplyOutcome::Applied => {
                        accepted_trace_records += 1;
                        "acceptedMemory".to_string()
                    }
                    ApplyOutcome::Duplicate { .. } => "duplicateReplay".to_string(),
                    ApplyOutcome::Rejected { reason } => map_reject_reason(reason),
                    ApplyOutcome::Quarantined { reason } => map_quarantine_reason(reason),
                };
                outcomes.push(outcome);
            }
            other => panic!("unsupported trace record kind: {other}"),
        }
    }

    let replica_summary = replica.summary();
    let mut replayed = ReplicaState::new(space_id, author_identity("root").expect("authority"));
    assert!(
        replayed
            .replay_authenticated(replica.applied_authenticated_records())
            .iter()
            .all(|outcome| *outcome == ApplyOutcome::Applied)
    );
    assert_eq!(
        replayed.summary(),
        replica_summary,
        "authenticated replay preserves publisher attribution"
    );
    let summary = TraceSummary {
        seen_request_count: accepted_trace_records,
        evidence_count: replica_summary.evidence_count,
        memory_version_count: replica_summary.memory_version_count,
        active_canonical_count: replica_summary.active_canonical_count,
        active_publishers: replica_summary.active_publishers,
        revoked_publishers: replica_summary.revoked_publishers,
        canonical_heads: replica_summary
            .canonical_heads
            .into_iter()
            .map(|head| TraceCanonicalHead {
                canonical_id: head.canonical_id,
                version: head.version,
                publisher: head.publisher,
                author: head.author,
                source: head.source,
                locator: head.locator,
                digest: head.digest,
                evidence_count: head.evidence_count,
                tombstone: head.tombstone,
            })
            .collect(),
    };

    (outcomes, summary, digest_mismatches)
}

fn membership_transport(
    space_id: &ProjectSpaceId,
    record: &TraceRecord,
    authored_at: chrono::DateTime<Utc>,
    is_grant: bool,
) -> Result<TransportRecord, codifix_federation::FederationError> {
    let subject = author_identity(record.subject.as_deref().expect("subject"))?;
    let capability = parse_capability(record.capability.as_deref().expect("capability"));
    let envelope = if is_grant {
        SharedRecordEnvelope::new(
            record_id(&record.request_id)?,
            space_id.clone(),
            author_identity(&record.authority)?,
            authored_at,
            None,
            FederationRecord::Membership(MembershipRecord {
                member: subject,
                capability,
            }),
        )
    } else {
        SharedRecordEnvelope::new(
            record_id(&record.request_id)?,
            space_id.clone(),
            author_identity(&record.authority)?,
            authored_at,
            None,
            FederationRecord::Revocation(RevocationRecord {
                member: subject,
                capability: Some(capability),
                reason: Some(record.request_id.clone()),
            }),
        )
    };

    TransportRecord::new(
        source_identity(record.source.as_deref().unwrap_or("control"))?,
        transport_locator(
            record
                .locator
                .as_deref()
                .unwrap_or(record.request_id.as_str()),
        )?,
        envelope,
    )
}

fn evidence_transport(
    space_id: &ProjectSpaceId,
    record: &TraceRecord,
    authored_at: chrono::DateTime<Utc>,
) -> Result<TransportRecord, codifix_federation::FederationError> {
    let evidence_id = record.evidence_id.as_deref().expect("evidence id");
    let source = record.source.as_deref().expect("source");
    let locator = record.locator.as_deref().expect("locator");
    let source_identity = source_identity(source)?;
    let author = author_identity(&record.authority)?;
    let transport_locator = transport_locator(locator)?;
    let envelope = SharedRecordEnvelope::new(
        record_id(&record.request_id)?,
        space_id.clone(),
        author,
        authored_at,
        None,
        FederationRecord::Evidence(codifix_federation::EvidenceRecord {
            evidence_id: RecordLocator::new(evidence_id)?,
            source_id: RecordLocator::new(format!("source-ref:{evidence_id}"))?,
            memory_id: RecordLocator::new("unbound-memory")?,
            canonical_id: RecordLocator::new("unbound-canonical")?,
            version_no: 1,
            source_kind: codifix_record::SourceKind::Note,
            task_id: None,
            file_path: None,
            git_commit: None,
            symbol_name: None,
            symbol_kind: None,
            excerpt: Some(format!("evidence {evidence_id}")),
            provenance: None,
            observed_at: authored_at,
        }),
    );
    TransportRecord::new(source_identity, transport_locator, envelope)
}

fn memory_transport(
    space_id: &ProjectSpaceId,
    record: &TraceRecord,
    authored_at: chrono::DateTime<Utc>,
) -> Result<TransportRecord, codifix_federation::FederationError> {
    let source = record.source.as_deref().expect("source");
    let source_identity = source_identity(source)?;
    let author = author_identity(record.author.as_deref().expect("author"))?;
    let locator = record.locator.as_deref().expect("locator");
    let canonical_id = RecordLocator::new(record.canonical_id.as_deref().expect("canonical id"))?;
    let version = record.version.expect("version");
    let memory_id = RecordLocator::new(format!("memory:{}", record.request_id))?;
    let memory_record_id = record_id(&record.request_id)?;
    let evidence_refs = record
        .evidence_refs
        .iter()
        .map(|value| RecordLocator::new(value.clone()))
        .collect::<Result<Vec<_>, _>>()?;

    TransportRecord::new(
        source_identity,
        transport_locator(locator)?,
        SharedRecordEnvelope::new(
            memory_record_id,
            space_id.clone(),
            author,
            authored_at,
            None,
            FederationRecord::MemoryVersion(MemoryVersionRecord {
                memory_id,
                canonical_id,
                version_no: version,
                canonical_text: format!(
                    "canonical {} v{}",
                    record.canonical_id.as_deref().expect("canonical id"),
                    version
                ),
                summary: format!(
                    "summary {} v{}",
                    record.canonical_id.as_deref().expect("canonical id"),
                    version
                ),
                memory_type: MemoryType::Decision,
                importance: 3,
                confidence: 0.8,
                status: if record.tombstone {
                    MemoryStatus::Archived
                } else {
                    MemoryStatus::Active
                },
                tags: vec![fixture_tag(record)],
                evidence_refs: evidence_refs.clone(),
                related_memories: Vec::new(),
                created_at: authored_at,
                updated_at: authored_at,
                is_tombstone: record.tombstone,
            }),
        ),
    )
}

fn map_simple_outcome(outcome: ApplyOutcome, accepted_label: &str) -> String {
    match outcome {
        ApplyOutcome::Applied => accepted_label.to_string(),
        ApplyOutcome::Duplicate { .. } => "duplicateReplay".to_string(),
        ApplyOutcome::Rejected { reason } => map_reject_reason(reason),
        ApplyOutcome::Quarantined { reason } => map_quarantine_reason(reason),
    }
}

fn map_reject_reason(reason: RejectReason) -> String {
    match reason {
        RejectReason::UnauthorizedMembershipChange { .. }
        | RejectReason::PublisherMismatch { .. }
        | RejectReason::UnauthorizedDataWrite { .. } => "unauthorized".to_string(),
        RejectReason::RevokedPublisher { .. } => "revokedPublisher".to_string(),
        RejectReason::Equivocation { .. } => "equivocation".to_string(),
        _ => "invalidShape".to_string(),
    }
}

fn map_quarantine_reason(reason: codifix_federation::QuarantineReason) -> String {
    match reason {
        QuarantineReason::VersionGap { .. } => "versionGap".to_string(),
        QuarantineReason::MissingEvidence { .. } => "missingEvidence".to_string(),
        _ => "invalidShape".to_string(),
    }
}

fn load_fixture(path: &PathBuf) -> TraceFixture {
    serde_json::from_str(&fs::read_to_string(path).expect("read fixture")).expect("parse fixture")
}

fn trace_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/federation/traces")
        .canonicalize()
        .expect("canonical trace dir")
}

fn parse_capability(raw: &str) -> SpaceCapability {
    match raw {
        "publish" => SpaceCapability::Publish,
        "pull" => SpaceCapability::Pull,
        "manage_membership" => SpaceCapability::ManageMembership,
        other => panic!("unsupported capability: {other}"),
    }
}

fn author_identity(raw: &str) -> Result<AuthorIdentity, codifix_federation::FederationError> {
    AuthorIdentity::new(raw)
}

fn source_identity(raw: &str) -> Result<SourceIdentity, codifix_federation::FederationError> {
    SourceIdentity::new(raw)
}

fn transport_locator(locator: &str) -> Result<RecordLocator, codifix_federation::FederationError> {
    RecordLocator::new(locator)
}

fn record_id(raw: &str) -> Result<RecordLocator, codifix_federation::FederationError> {
    RecordLocator::new(format!("record:{raw}"))
}

fn fixture_tag(record: &TraceRecord) -> String {
    format!("fixture:{}", record.request_id)
}
