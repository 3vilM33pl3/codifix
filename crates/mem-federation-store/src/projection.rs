// SPDX-License-Identifier: AGPL-3.0-or-later
use std::collections::BTreeSet;

use mem_federation::{SpaceCapability, publication::*};
use serde_json::{Value, json};

/// Persist only derived views; replay, not deserialization of internal reducer
/// indexes, remains authoritative. Arrays have deterministic semantic ordering.
pub(crate) fn snapshot(state: &PublicationState) -> Value {
    let mut members = BTreeSet::from([state.authority()]);
    let mut streams = BTreeSet::new();
    let mut evidence = BTreeSet::new();
    let mut relations = BTreeSet::new();
    let mut proposals = BTreeSet::new();
    for delivery in state.accepted() {
        match &delivery.envelope.record {
            PublicationRecord::Membership { member, .. }
            | PublicationRecord::Revocation { member, .. } => {
                members.insert(*member);
            }
            PublicationRecord::Evidence(_) => {
                evidence.insert(delivery.envelope.key());
            }
            PublicationRecord::MemoryVersion { stream, .. }
            | PublicationRecord::CanonicalState { stream, .. } => {
                streams.insert(*stream);
            }
            PublicationRecord::Relation(relation) => {
                relations.insert(RecordKey {
                    publisher: relation.source.origin,
                    id: relation.id,
                });
            }
            PublicationRecord::Proposal(proposal) => {
                proposals.insert(delivery.envelope.key());
                streams.insert(proposal.target);
            }
            PublicationRecord::ResolveProposal { .. } => {}
            PublicationRecord::Expertise(_) => {}
        }
    }
    let mut value = json!({
        "sequence": state.sequence(),
        "memberships": members.into_iter().map(|member| json!({
            "member": member,
            "capabilities": ([SpaceCapability::Pull, SpaceCapability::Publish, SpaceCapability::ManageMembership,
                SpaceCapability::AskExpertise, SpaceCapability::AnswerExpertise]
                .into_iter().filter(|cap| state.has_capability(member, *cap)).collect::<Vec<_>>())
        })).collect::<Vec<_>>(),
        "streams": streams.into_iter().map(|stream| json!({
            "stream": stream,
            "versions": state.history(stream).iter().map(|version| json!({
                "key": version.key, "publication": version.publication,
                "author": version.author, "authored_at": version.authored_at
            })).collect::<Vec<_>>(),
            "states": state.state_history(stream), "current_state": state.current_state(stream)
        })).collect::<Vec<_>>(),
        "evidence": evidence.into_iter().map(|key| json!({"key": key, "snapshot": state.evidence(key)})).collect::<Vec<_>>(),
        "relations": relations.into_iter().map(|key| json!({"key": key, "history": state.relation_history(key)})).collect::<Vec<_>>(),
        "proposals": proposals.into_iter().filter_map(|key| state.proposal(key).map(|p| json!({
            "key": key, "proposal": p.proposal, "author": p.author, "authored_at": p.authored_at,
            "resolution": match &p.resolution {
                ProposalResolution::Pending => json!({"status": "pending"}),
                ProposalResolution::Accepted {by, version} => json!({"status":"accepted", "by": by, "version": version}),
                ProposalResolution::Rejected {by} => json!({"status":"rejected", "by": by}),
            }
        }))).collect::<Vec<_>>()
    });
    // Do not change persisted v2 snapshots merely by upgrading the executable.
    let expertise = state.expertise_catalog().collect::<Vec<_>>();
    if !expertise.is_empty() {
        value["expertise"] = json!(expertise);
    }
    value
}
