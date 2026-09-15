// SPDX-License-Identifier: AGPL-3.0-or-later
//! Post-execution review. Never changes provider usage, timing, inputs or calls.
use super::{
    Artifact, Manifest, digest_value,
    scoring::{self, Ballot, ReviewPacket, SupersededBallot},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BallotUpdate {
    pub ballot: Ballot,
    /// Required to revise an existing reviewer's judgment. The old ballot is
    /// retained, linked to the new one; another reviewer's dissent is never erased.
    pub replaces_sha256: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewBatch {
    pub artifact_sha256: String,
    pub updates: Vec<BallotUpdate>,
}

fn validate(manifest: &Manifest, artifact: &Artifact) -> Result<()> {
    let report = super::statistics::report_for_review(manifest, artifact)?;
    ensure!(
        report.harness_valid,
        "invalid execution evidence; review cannot repair it: {}",
        report.errors.join("; ")
    );
    Ok(())
}

pub fn prepare(manifest: &Manifest, artifact: &Artifact) -> Result<Vec<ReviewPacket>> {
    validate(manifest, artifact)?;
    let mut packets = BTreeMap::new();
    for observation in &artifact.observations {
        if let Some(evidence) = &observation.knowledge_review {
            ensure!(
                packets
                    .insert(evidence.packet.review_id.clone(), evidence.packet.clone())
                    .is_none(),
                "duplicate blinded review identity"
            );
        }
    }
    // UUID ordering is independent of condition order; no mapping is exported.
    Ok(packets.into_values().collect())
}

pub fn apply(manifest: &Manifest, artifact: &Artifact, batch: &ReviewBatch) -> Result<Artifact> {
    prepare(manifest, artifact)?;
    ensure!(
        batch.artifact_sha256 == digest_value(artifact)?,
        "review batch belongs to another artifact"
    );
    ensure!(
        !batch.updates.is_empty() && batch.updates.len() <= 10000,
        "invalid review batch size"
    );
    let mut updated = artifact.clone();
    let mut packets = BTreeMap::new();
    for (i, o) in updated.observations.iter().enumerate() {
        if let Some(e) = &o.knowledge_review {
            ensure!(
                packets.insert(digest_value(&e.packet)?, i).is_none(),
                "duplicate review packet"
            );
        }
    }
    let mut seen = BTreeSet::new();
    for update in &batch.updates {
        let ballot = &update.ballot;
        ensure!(
            seen.insert((&ballot.packet_sha256, &ballot.reviewer)),
            "duplicate reviewer in batch"
        );
        let index = *packets
            .get(&ballot.packet_sha256)
            .ok_or_else(|| anyhow::anyhow!("unknown review packet"))?;
        let observation = &mut updated.observations[index];
        let evidence = observation
            .knowledge_review
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("missing review evidence"))?;
        scoring::assess(&evidence.packet, std::slice::from_ref(ballot))?;
        if let Some(previous) = evidence
            .ballots
            .iter_mut()
            .find(|b| b.reviewer == ballot.reviewer)
        {
            let old_hash = digest_value(previous)?;
            let new_hash = digest_value(ballot)?;
            if new_hash != old_hash {
                ensure!(
                    update.replaces_sha256.as_deref() == Some(&old_hash)
                        && previous.kind == ballot.kind,
                    "review revision requires exact predecessor and same reviewer kind"
                );
                evidence.history.push(SupersededBallot {
                    ballot: previous.clone(),
                    replacement_sha256: new_hash,
                });
                *previous = ballot.clone();
            } else {
                ensure!(
                    update
                        .replaces_sha256
                        .as_ref()
                        .is_none_or(|h| h == &old_hash),
                    "invalid idempotent review predecessor"
                );
            }
        } else {
            ensure!(
                update.replaces_sha256.is_none(),
                "new reviewer cannot supersede another reviewer"
            );
            evidence.ballots.push(ballot.clone());
        }
        let assessment = scoring::assess_evidence(evidence)?;
        observation.result.success = assessment.success;
        observation.unresolved_rubric = assessment.unresolved;
    }
    validate(manifest, &updated)?;
    Ok(updated)
}
