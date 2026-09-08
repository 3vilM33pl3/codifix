// SPDX-License-Identifier: AGPL-3.0-or-later
//! Normalized commands shared with Lean, expanded into the real publication
//! reducer. These fixtures are not a wire format or a cryptography model.

use mem_federation::{AuthorIdentity, RecordLocator, SpaceCapability, publication::*};
use mem_record::{MemoryStatus, MemoryType};
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
    streams: Vec<Summary>,
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
    fn delivery(&self) -> PublicationDelivery {
        // Keep the normalized representation injective: unused command fields
        // must be default, so Lean's record equality matches expanded deliveries.
        match self.kind.as_str() {
            "grant" | "revoke" => {
                assert_eq!(
                    (
                        self.origin,
                        self.canonical,
                        self.shared,
                        self.local,
                        self.revision
                    ),
                    (0, 0, 0, 0, 0)
                );
                assert!(!self.tombstone);
                assert_eq!(self.text, knowledge());
            }
            "memory" => {
                assert_eq!((self.subject, self.revision), (0, 0));
                assert!(!self.tombstone);
            }
            "state" => {
                assert_eq!((self.subject, self.local), (0, 0));
                assert_eq!(self.text, knowledge());
            }
            other => panic!("unknown fixture command: {other}"),
        }
        let confidence = ConfidencePpm::try_from(800_000).expect("valid fixture confidence");
        let record = match self.kind.as_str() {
            "grant" => PublicationRecord::Membership {
                member: instance(self.subject),
                capability: SpaceCapability::Publish,
            },
            "revoke" => PublicationRecord::Revocation {
                member: instance(self.subject),
                capability: Some(SpaceCapability::Publish),
            },
            "memory" => PublicationRecord::MemoryVersion {
                stream: stream(self.origin, self.canonical),
                publication: MemoryPublication {
                    version: SharedVersion::new(self.shared).expect("positive shared version"),
                    source: SourceVersionRef {
                        record_id: record_id(999),
                        version: SourceVersion::new(self.local).expect("positive local version"),
                    },
                    content: MemoryContent {
                        text: self.text.clone(),
                        summary: self.text.clone(),
                        memory_type: MemoryType::Decision,
                        confidence,
                        importance: 3,
                        tags: vec![],
                    },
                    evidence: vec![],
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
            _ => unreachable!(),
        };
        PublicationDelivery::new(
            AuthoritySequence::new(self.sequence).expect("positive accepted order"),
            RecordLocator::new(format!("record:{}", self.id)).expect("valid fixture locator"),
            PublicationEnvelope {
                schema_version: PUBLICATION_SCHEMA_VERSION,
                space: space(),
                publisher: instance(self.publisher),
                record_id: record_id(self.id),
                author: AuthorIdentity::new(&self.author).expect("valid fixture author"),
                authored_at: "2026-09-08T00:00:00Z".parse().expect("valid fixture time"),
                causation: None,
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
    ] {
        let fixture: Fixture = serde_json::from_str(raw).expect("valid shared fixture JSON");
        let mut state = PublicationState::new(space(), instance(1));
        let mut outcomes = Vec::new();
        for command in &fixture.records {
            let before = state.clone();
            let outcome = state.apply(instance(command.authenticated), command.delivery());
            if outcome != PublicationOutcome::Applied {
                assert_eq!(
                    state, before,
                    "{}: failed delivery changed state",
                    fixture.name
                );
            }
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
        members.retain(|id| state.has_capability(instance(*id), SpaceCapability::Publish));
        assert_eq!(members, fixture.publishers);
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
                },
                *expected
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
