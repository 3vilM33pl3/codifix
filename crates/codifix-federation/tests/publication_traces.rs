// SPDX-License-Identifier: AGPL-3.0-or-later
//! Normalized commands shared with Lean, expanded into the real publication
//! reducer. These fixtures are not a wire format or a cryptography model.

use codifix_federation::{
    AuthorIdentity, CanonicalJsonCodec, ContentDigest, RecordLocator, SpaceCapability,
    publication::*,
};
use codifix_record::{MemoryStatus, MemoryType};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    name: String,
    records: Vec<Command>,
    outcomes: Vec<String>,
    sequence: u32,
    publishers: Vec<u32>,
    pullers: Vec<u32>,
    managers: Vec<u32>,
    streams: Vec<Summary>,
    evidence: Vec<EvidenceExpected>,
    proposals: Vec<ProposalExpected>,
    relations: Vec<RelationExpected>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Key {
    publisher: u32,
    id: u32,
}
impl Key {
    fn record_key(self) -> RecordKey {
        RecordKey {
            publisher: instance(self.publisher),
            id: record_id(self.id),
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceExpected {
    publisher: u32,
    id: u32,
    text: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalExpected {
    publisher: u32,
    id: u32,
    status: String,
    version: u32,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RelationExpected {
    publisher: u32,
    id: u32,
    revisions: Vec<u32>,
    active: Vec<bool>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Summary {
    origin: u32,
    canonical: u32,
    shared: u32,
    local: u32,
    revision: u32,
    author: String,
    current: bool,
    tombstone: bool,
    text: String,
    evidence: Vec<Key>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    sequence: u32,
    id: u32,
    publisher: u32,
    authenticated: u32,
    kind: String,
    #[serde(default)]
    subject: u32,
    #[serde(default)]
    origin: u32,
    #[serde(default)]
    canonical: u32,
    #[serde(default)]
    shared: u32,
    #[serde(default)]
    local: u32,
    #[serde(default)]
    revision: u32,
    #[serde(default = "expert")]
    author: String,
    #[serde(default = "knowledge")]
    text: String,
    #[serde(default)]
    tombstone: bool,
    #[serde(default = "publish")]
    capability: String,
    #[serde(default)]
    causation: Option<Key>,
    #[serde(default)]
    evidence: Vec<Key>,
    #[serde(default = "space_number", rename = "evidenceSpace")]
    evidence_space: u32,
    #[serde(default)]
    proposal: Option<Key>,
    #[serde(default, rename = "relationId")]
    relation_id: u32,
    #[serde(default, rename = "targetOrigin")]
    target_origin: u32,
    #[serde(default, rename = "targetCanonical")]
    target_canonical: u32,
    #[serde(default = "related_to", rename = "relationType")]
    relation_type: String,
    #[serde(default = "yes")]
    active: bool,
}
fn publish() -> String {
    "publish".into()
}
fn space_number() -> u32 {
    100
}
fn related_to() -> String {
    "related_to".into()
}
fn yes() -> bool {
    true
}
fn capability(value: &str) -> Option<SpaceCapability> {
    match value {
        "" => None,
        "publish" => Some(SpaceCapability::Publish),
        "pull" => Some(SpaceCapability::Pull),
        "manage_membership" => Some(SpaceCapability::ManageMembership),
        "ask_expertise" => Some(SpaceCapability::AskExpertise),
        "answer_expertise" => Some(SpaceCapability::AnswerExpertise),
        _ => panic!("invalid fixture capability"),
    }
}
fn expert() -> String {
    "expert".into()
}
fn knowledge() -> String {
    "knowledge".into()
}
fn instance(id: u32) -> InstanceId {
    InstanceId::new(Uuid::from_u128(id.into())).expect("valid fixture instance")
}
fn record_id(id: u32) -> RecordId {
    RecordId::new(Uuid::from_u128(id.into())).expect("valid fixture record ID")
}
fn space() -> SpaceId {
    SpaceId::new(Uuid::from_u128(100)).expect("valid fixture space")
}
fn stream(origin: u32, canonical: u32) -> StreamId {
    StreamId {
        origin: instance(origin),
        canonical: record_id(canonical),
    }
}

impl Command {
    fn content(&self) -> MemoryContent {
        MemoryContent {
            text: self.text.clone(),
            summary: self.text.clone(),
            memory_type: MemoryType::Decision,
            confidence: ConfidencePpm::try_from(800_000).expect("valid confidence"),
            importance: 3,
            tags: vec![],
        }
    }
    fn source(&self) -> SourceVersionRef {
        SourceVersionRef {
            record_id: record_id(999),
            version: SourceVersion::new(self.local).expect("positive source version"),
        }
    }
    fn delivery(&self) -> PublicationDelivery {
        // Keep the normalized representation injective: unused command fields
        // must be default, so Lean's record equality matches expanded deliveries.
        let kind = self.kind.as_str();
        if !matches!(kind, "grant" | "revoke") {
            assert_eq!(self.subject, 0);
            assert_eq!(self.capability, publish());
        }
        if !matches!(
            kind,
            "memory" | "state" | "evidence" | "proposal" | "relation" | "expertise"
        ) {
            assert_eq!((self.origin, self.canonical), (0, 0));
        }
        if !matches!(kind, "memory" | "state" | "proposal") {
            assert_eq!(self.shared, 0);
        }
        if !matches!(kind, "memory" | "resolve") {
            assert_eq!(self.local, 0);
        }
        if !matches!(kind, "state" | "relation" | "expertise") {
            assert_eq!(self.revision, 0);
        }
        if !matches!(kind, "memory" | "proposal" | "evidence" | "expertise") {
            assert_eq!(self.text, knowledge());
        }
        if kind != "state" {
            assert!(!self.tombstone);
        }
        if !matches!(kind, "memory" | "proposal") {
            assert!(self.evidence.is_empty());
        }
        if kind != "evidence" {
            assert_eq!(self.evidence_space, 100);
        }
        if kind != "resolve" {
            assert!(self.proposal.is_none());
        }
        if kind != "relation" {
            assert_eq!(
                (self.relation_id, self.target_origin, self.target_canonical),
                (0, 0, 0)
            );
            assert_eq!(self.relation_type, related_to());
            if kind != "expertise" {
                assert!(self.active);
            }
        }
        let confidence = ConfidencePpm::try_from(800_000).expect("valid fixture confidence");
        let record = match self.kind.as_str() {
            "expertise" => PublicationRecord::Expertise(
                codifix_federation::expertise::CapabilityAdvertisement {
                    owner: instance(self.origin),
                    collection: record_id(self.canonical),
                    revision: StateRevision::new(self.revision)
                        .expect("positive expertise revision"),
                    collection_digest: CanonicalJsonCodec::digest(&self.text)
                        .expect("fixture digest"),
                    name: self.text.clone(),
                    description: String::new(),
                    topics: vec![],
                    modes: vec![codifix_federation::expertise::AnswerMode::Deterministic],
                    requesters: vec![instance(3)],
                    model_destinations: vec![],
                    output_destinations: vec!["local_human".into()],
                    active: self.active,
                    limits: Default::default(),
                },
            ),
            "grant" => PublicationRecord::Membership {
                member: instance(self.subject),
                capability: capability(&self.capability).expect("grant requires a capability"),
            },
            "revoke" => PublicationRecord::Revocation {
                member: instance(self.subject),
                capability: capability(&self.capability),
            },
            "memory" => PublicationRecord::MemoryVersion {
                stream: stream(self.origin, self.canonical),
                publication: MemoryPublication {
                    version: SharedVersion::new(self.shared).expect("positive shared version"),
                    source: self.source(),
                    content: self.content(),
                    evidence: self.evidence.iter().map(|k| k.record_key()).collect(),
                },
            },
            "state" => PublicationRecord::CanonicalState {
                stream: stream(self.origin, self.canonical),
                state: StatePublication {
                    revision: StateRevision::new(self.revision).expect("positive state revision"),
                    content_version: SharedVersion::new(self.shared)
                        .expect("positive content version"),
                    status: MemoryStatus::Active,
                    confidence,
                    tombstone: self.tombstone,
                },
            },
            "evidence" => {
                let snapshot = EvidenceSnapshot {
                    space: SpaceId::new(Uuid::from_u128(self.evidence_space.into()))
                        .expect("valid evidence space"),
                    stream: stream(self.origin, self.canonical),
                    public_source_label: "reviewed-extract".into(),
                    selected_text: self.text.clone(),
                    observed_at: "2026-09-08T00:00:00Z".parse().expect("valid time"),
                };
                let receipt = ReviewReceipt {
                    snapshot_digest: CanonicalJsonCodec::digest(&snapshot)
                        .expect("snapshot digest"),
                    reviewer: AuthorIdentity::new("reviewer").expect("valid reviewer"),
                    reviewed_at: snapshot.observed_at,
                };
                PublicationRecord::Evidence(
                    ReviewedEvidence::new(snapshot, receipt).expect("reviewed snapshot"),
                )
            }
            "proposal" => PublicationRecord::Proposal(CorrectionProposal {
                target: stream(self.origin, self.canonical),
                base_version: SharedVersion::new(self.shared).expect("positive base"),
                content: self.content(),
                evidence: self.evidence.iter().map(|k| k.record_key()).collect(),
            }),
            "resolve" => PublicationRecord::ResolveProposal {
                proposal: self.proposal.expect("proposal key").record_key(),
                source: (self.local != 0).then(|| self.source()),
            },
            "relation" => PublicationRecord::Relation(RelationPublication {
                id: record_id(self.relation_id),
                source: stream(self.origin, self.canonical),
                target: stream(self.target_origin, self.target_canonical),
                relation_type: serde_json::from_value(serde_json::json!(&self.relation_type))
                    .expect("valid relation type"),
                revision: StateRevision::new(self.revision).expect("positive revision"),
                active: self.active,
            }),
            _ => panic!("unknown fixture kind"),
        };
        PublicationDelivery::new(
            AuthoritySequence::new(self.sequence).expect("positive accepted order"),
            RecordLocator::new(format!("record:{}", self.id)).expect("valid fixture locator"),
            PublicationEnvelope {
                schema_version: if kind == "expertise"
                    || matches!(
                        self.capability.as_str(),
                        "ask_expertise" | "answer_expertise"
                    ) {
                    3
                } else {
                    PUBLICATION_SCHEMA_VERSION
                },
                space: space(),
                publisher: instance(self.publisher),
                record_id: record_id(self.id),
                author: AuthorIdentity::new(&self.author).expect("valid fixture author"),
                authored_at: "2026-09-08T00:00:00Z".parse().expect("valid fixture time"),
                causation: self.causation.map(Key::record_key),
                record,
            },
        )
        .expect("canonical fixture delivery")
    }
}

#[test]
fn publication_rust_lean_traces_use_real_reducer() {
    for raw in [
        include_str!("../../../fixtures/federation/publication-traces/lifecycle.json"),
        include_str!("../../../fixtures/federation/publication-traces/order-and-ownership.json"),
        include_str!("../../../fixtures/federation/publication-traces/evidence-and-proposals.json"),
        include_str!(
            "../../../fixtures/federation/publication-traces/relations-and-causation.json"
        ),
        include_str!("../../../fixtures/federation/publication-traces/capabilities.json"),
        include_str!("../../../fixtures/federation/publication-traces/expertise.json"),
    ] {
        let fixture: Fixture = serde_json::from_str(raw).expect("valid shared fixture JSON");
        let mut state = PublicationState::new(space(), instance(1));
        let mut outcomes = Vec::new();
        let mut accepted = Vec::new();
        for command in &fixture.records {
            let before = state.clone();
            let delivery = command.delivery();
            let decoded: PublicationDelivery = serde_json::from_value(
                serde_json::to_value(&delivery).expect("serialized delivery"),
            )
            .expect("validated delivery");
            assert_eq!(decoded, delivery);
            let outcome = state.apply(instance(command.authenticated), decoded);
            if outcome != PublicationOutcome::Applied {
                assert_eq!(
                    state, before,
                    "{}: failed delivery changed state",
                    fixture.name
                );
            } else {
                accepted.push(delivery);
            }
            assert_eq!(state.accepted(), accepted);
            let json = serde_json::to_value(outcome).expect("serializable outcome");
            outcomes.push(
                json.get("reason")
                    .unwrap_or(&json["outcome"])
                    .as_str()
                    .expect("string outcome tag")
                    .to_owned(),
            );
        }
        assert_eq!(outcomes, fixture.outcomes, "{}", fixture.name);
        assert_eq!(state.sequence(), fixture.sequence);
        let mut members: Vec<_> = fixture
            .records
            .iter()
            .flat_map(|r| [r.publisher, r.subject])
            .filter(|id| *id != 0)
            .collect();
        members.sort();
        members.dedup();
        for (capability, expected) in [
            (SpaceCapability::Publish, &fixture.publishers),
            (SpaceCapability::Pull, &fixture.pullers),
            (SpaceCapability::ManageMembership, &fixture.managers),
        ] {
            let actual: Vec<_> = members
                .iter()
                .copied()
                .filter(|id| state.has_capability(instance(*id), capability))
                .collect();
            assert_eq!(&actual, expected);
        }
        for expected in &fixture.streams {
            let stream = stream(expected.origin, expected.canonical);
            let latest = state
                .history(stream)
                .last()
                .expect("expected published stream");
            assert_eq!(
                Summary {
                    origin: expected.origin,
                    canonical: expected.canonical,
                    shared: state.content_version(stream),
                    local: latest.publication.source.version.get(),
                    revision: state.state_revision(stream),
                    author: latest.author.to_string(),
                    current: state.current_state(stream).is_some(),
                    tombstone: state.current_state(stream).is_some_and(|s| s.tombstone),
                    text: latest.publication.content.text.clone(),
                    evidence: latest
                        .publication
                        .evidence
                        .iter()
                        .map(|k| Key {
                            publisher: u32::try_from(k.publisher.as_uuid().as_u128())
                                .expect("fixture ID"),
                            id: u32::try_from(k.id.as_uuid().as_u128()).expect("fixture ID"),
                        })
                        .collect(),
                },
                *expected
            );
        }
        for expected in fixture.evidence {
            let evidence = state
                .evidence(
                    Key {
                        publisher: expected.publisher,
                        id: expected.id,
                    }
                    .record_key(),
                )
                .expect("accepted evidence");
            assert_eq!(evidence.snapshot().selected_text, expected.text);
            assert_eq!(
                ContentDigest::sha256_bytes(
                    &evidence.published_snapshot_bytes().expect("snapshot bytes")
                ),
                evidence.review().snapshot_digest
            );
        }
        for expected in fixture.proposals {
            let proposal = state
                .proposal(
                    Key {
                        publisher: expected.publisher,
                        id: expected.id,
                    }
                    .record_key(),
                )
                .expect("accepted proposal");
            let (status, version) = match proposal.resolution {
                ProposalResolution::Pending => ("pending", 0),
                ProposalResolution::Accepted { version, .. } => ("accepted", version.get()),
                ProposalResolution::Rejected { .. } => ("rejected", 0),
            };
            assert_eq!(
                (status, version),
                (expected.status.as_str(), expected.version)
            );
        }
        for expected in fixture.relations {
            let history = state.relation_history(
                Key {
                    publisher: expected.publisher,
                    id: expected.id,
                }
                .record_key(),
            );
            assert_eq!(
                history.iter().map(|r| r.revision.get()).collect::<Vec<_>>(),
                expected.revisions
            );
            assert_eq!(
                history.iter().map(|r| r.active).collect::<Vec<_>>(),
                expected.active
            );
        }
        let mut replayed = PublicationState::new(space(), instance(1));
        replayed.replay(
            fixture
                .records
                .iter()
                .map(|r| (instance(r.authenticated), r.delivery())),
        );
        assert_eq!(replayed, state, "{}: fold/replay diverged", fixture.name);
    }
}
