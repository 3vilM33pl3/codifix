// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private knowledge adjudication. Exact quotes establish source-byte identity,
//! not entailment. Semantic judgments require operator-collected human ballots;
//! a model diagnosis is never a passing ballot. These are integrity bindings,
//! not signatures or a replacement for authenticating the reviewing operator.
use super::{
    AssetKind, Manifest, Observation, Track, digest, digest_value, identifier, valid_digest,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    FactualCorrectness,
    RequiredEvidence,
    AppropriateRefusal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub id: String,
    pub dimension: Dimension,
    /// Expected facts, acceptable alternatives and prohibited unsupported claims.
    pub instruction: String,
    pub critical: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rubric {
    pub case_id: String,
    pub question: String,
    pub criteria: Vec<Criterion>,
    pub minimum_citations: usize,
    pub citations_critical: bool,
}

/// Label assets use compact struct-field-order JSON followed by one LF. This
/// makes the manifest's raw file digest identical to the reviewed rubric digest.
pub fn rubric_bytes(rubric: &Rubric) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(rubric)?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Campaign-relative source asset, not a local host path or endpoint.
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    pub source_id: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub text: String,
    pub refused: bool,
    pub citations: Vec<Citation>,
}

/// No condition, repeat, model, score or automated judgment is exposed here.
/// The host retains the private mapping from opaque review ID to execution.
/// Answer text may itself reveal a condition; procedural blinding is imperfect.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewPacket {
    pub version: u32,
    pub review_id: String,
    pub manifest_sha256: String,
    /// Salted execution commitment. The salt/work mapping stays with the host,
    /// so reviewers cannot enumerate the five conditions from an unsalted hash.
    pub execution_sha256: String,
    pub rubric: Rubric,
    pub sources: Vec<Source>,
    pub answer: Answer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Pass,
    Fail,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewerKind {
    Human,
    ModelDiagnosis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Judgment {
    pub decision: Decision,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ballot {
    pub packet_sha256: String,
    pub reviewer: String,
    pub kind: ReviewerKind,
    /// Every criterion must be addressed exactly once; no default pass.
    pub judgments: BTreeMap<String, Judgment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeEvidence {
    pub packet: ReviewPacket,
    pub execution_nonce: String,
    pub ballots: Vec<Ballot>,
    #[serde(default)]
    pub history: Vec<SupersededBallot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupersededBallot {
    pub ballot: Ballot,
    pub replacement_sha256: String,
}

impl KnowledgeEvidence {
    pub fn prepare(
        manifest_sha256: String,
        work: &super::Work,
        rubric: Rubric,
        sources: Vec<Source>,
        answer: Answer,
    ) -> Result<Self> {
        ensure!(rubric.case_id == work.case_id, "rubric/work mismatch");
        let execution_nonce = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let packet = ReviewPacket {
            version: 1,
            review_id: uuid::Uuid::new_v4().to_string(),
            manifest_sha256,
            execution_sha256: digest_value(&(&execution_nonce, work))?,
            rubric,
            sources,
            answer,
        };
        packet.validate()?;
        Ok(Self {
            packet,
            execution_nonce,
            ballots: vec![],
            history: vec![],
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assessment {
    pub packet_sha256: String,
    pub success: bool,
    pub unresolved: bool,
    pub critical_failure: bool,
    pub citations_valid: bool,
    pub criteria: BTreeMap<String, Decision>,
}

impl ReviewPacket {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && identifier(&self.review_id),
            "invalid review identity"
        );
        ensure!(
            valid_digest(&self.manifest_sha256),
            "invalid campaign digest"
        );
        ensure!(
            valid_digest(&self.execution_sha256),
            "invalid execution digest"
        );
        ensure!(identifier(&self.rubric.case_id), "invalid rubric case");
        ensure!(
            !self.rubric.question.trim().is_empty() && self.rubric.question.len() <= 16384,
            "invalid rubric question"
        );
        ensure!(
            !self.rubric.criteria.is_empty() && self.rubric.criteria.len() <= 64,
            "invalid criterion count"
        );
        let mut ids = BTreeSet::new();
        for criterion in &self.rubric.criteria {
            ensure!(
                identifier(&criterion.id)
                    && ids.insert(&criterion.id)
                    && !criterion.instruction.trim().is_empty()
                    && criterion.instruction.len() <= 16384,
                "invalid/duplicate criterion"
            );
        }
        for dimension in [
            Dimension::FactualCorrectness,
            Dimension::RequiredEvidence,
            Dimension::AppropriateRefusal,
        ] {
            ensure!(
                self.rubric
                    .criteria
                    .iter()
                    .any(|c| c.dimension == dimension),
                "missing scoring dimension"
            );
        }
        ensure!(
            !self.sources.is_empty() && self.sources.len() <= 128,
            "invalid source count"
        );
        let mut sources = BTreeSet::new();
        let mut bytes = 0usize;
        for source in &self.sources {
            ensure!(
                source.id.len() <= 1024
                    && super::relative_path(std::path::Path::new(&source.id))
                    && sources.insert(&source.id)
                    && !source.text.is_empty(),
                "invalid/duplicate source"
            );
            bytes = bytes.saturating_add(source.text.len());
        }
        ensure!(bytes <= 1024 * 1024, "oversized source packet");
        ensure!(
            self.answer.text.len() <= 65536
                && self.answer.citations.len() <= 32
                && self.rubric.minimum_citations <= 32,
            "oversized answer/citations"
        );
        for citation in &self.answer.citations {
            ensure!(
                citation.source_id.len() <= 1024 && citation.quote.len() <= 16384,
                "oversized citation"
            );
        }
        Ok(())
    }

    pub fn blank_ballot(&self, reviewer: String) -> Result<Ballot> {
        self.validate()?;
        ensure!(identifier(&reviewer), "invalid reviewer");
        Ok(Ballot {
            packet_sha256: digest_value(self)?,
            reviewer,
            kind: ReviewerKind::Human,
            judgments: self
                .rubric
                .criteria
                .iter()
                .map(|c| {
                    (
                        c.id.clone(),
                        Judgment {
                            decision: Decision::Unresolved,
                            reason: "Pending human review".into(),
                        },
                    )
                })
                .collect(),
        })
    }
}

pub fn assess(packet: &ReviewPacket, ballots: &[Ballot]) -> Result<Assessment> {
    packet.validate()?;
    ensure!(ballots.len() <= 32, "too many ballots");
    let packet_sha256 = digest_value(packet)?;
    let expected: BTreeSet<_> = packet.rubric.criteria.iter().map(|c| &c.id).collect();
    let mut reviewers = BTreeSet::new();
    for ballot in ballots {
        ensure!(
            ballot.packet_sha256 == packet_sha256,
            "ballot belongs to different evidence"
        );
        ensure!(
            identifier(&ballot.reviewer) && reviewers.insert(&ballot.reviewer),
            "duplicate/invalid reviewer"
        );
        ensure!(
            ballot.judgments.keys().collect::<BTreeSet<_>>() == expected,
            "incomplete/unknown judgments"
        );
        ensure!(
            ballot
                .judgments
                .values()
                .all(|j| !j.reason.trim().is_empty() && j.reason.len() <= 4096),
            "missing/oversized judgment reason"
        );
    }
    let mut citations = BTreeSet::new();
    let citations_valid = packet.answer.citations.iter().all(|citation| {
        !citation.quote.trim().is_empty()
            && citations.insert((&citation.source_id, &citation.quote))
            && packet.sources.iter().any(|source| {
                source.id == citation.source_id && source.text.contains(&citation.quote)
            })
    }) && citations.len() >= packet.rubric.minimum_citations;
    let mut criteria = BTreeMap::new();
    for criterion in &packet.rubric.criteria {
        let mut decisions = ballots
            .iter()
            .filter(|b| b.kind == ReviewerKind::Human)
            .map(|b| b.judgments[&criterion.id].decision);
        let first = decisions.next().unwrap_or(Decision::Unresolved);
        let decision = if decisions.all(|d| d == first) {
            first
        } else {
            Decision::Unresolved
        };
        criteria.insert(criterion.id.clone(), decision);
    }
    let unresolved = criteria.values().any(|d| *d == Decision::Unresolved);
    let critical_failure = (!citations_valid && packet.rubric.citations_critical)
        || packet
            .rubric
            .criteria
            .iter()
            .any(|c| c.critical && criteria[&c.id] == Decision::Fail);
    Ok(Assessment {
        packet_sha256,
        success: !packet.answer.text.trim().is_empty()
            && citations_valid
            && criteria.values().all(|d| *d == Decision::Pass),
        unresolved,
        critical_failure,
        citations_valid,
        criteria,
    })
}

/// Official evidence must carry its review; older offline smoke artifacts remain
/// readable. Whenever evidence is supplied, even offline, verify all bindings.
pub fn validate_observation(
    manifest: &Manifest,
    observation: &Observation,
    official: bool,
) -> Result<()> {
    validate_with_hash(manifest, &digest_value(manifest)?, observation, official)
}

pub(super) fn validate_with_hash(
    manifest: &Manifest,
    manifest_hash: &str,
    observation: &Observation,
    official: bool,
) -> Result<()> {
    validate_for_review_with_hash(manifest, manifest_hash, observation, official, false)
}

pub fn validate_pending_observation(manifest: &Manifest, observation: &Observation) -> Result<()> {
    validate_for_review_with_hash(manifest, &digest_value(manifest)?, observation, false, true)
}

pub(super) fn validate_for_review_with_hash(
    manifest: &Manifest,
    manifest_hash: &str,
    observation: &Observation,
    official: bool,
    allow_pending: bool,
) -> Result<()> {
    let case = manifest
        .cases
        .iter()
        .find(|c| c.id == observation.work.case_id)
        .ok_or_else(|| anyhow::anyhow!("unknown scoring case"))?;
    let Some(evidence) = &observation.knowledge_review else {
        ensure!(
            !observation.unresolved_rubric,
            "pending rubric without review evidence"
        );
        ensure!(
            !official || case.track != Track::Knowledge,
            "missing independent knowledge review"
        );
        return Ok(());
    };
    ensure!(
        case.track == Track::Knowledge,
        "knowledge review on coding case"
    );
    let packet = &evidence.packet;
    ensure!(
        valid_digest(&evidence.execution_nonce)
            && packet.execution_sha256
                == digest_value(&(&evidence.execution_nonce, &observation.work))?,
        "review execution mismatch"
    );
    ensure!(
        packet.manifest_sha256 == manifest_hash && packet.rubric.case_id == case.id,
        "review campaign/case mismatch"
    );
    ensure!(
        observation.result.answer.as_deref() == Some(packet.answer.text.as_str()),
        "review answer mismatch"
    );
    let rubric_hash = digest(&rubric_bytes(&packet.rubric)?);
    ensure!(
        manifest
            .assets
            .iter()
            .any(|a| a.kind == AssetKind::Labels && a.sha256 == rubric_hash),
        "rubric is not a locked label asset"
    );
    let source_ids: BTreeSet<_> = packet
        .sources
        .iter()
        .map(|s| std::path::Path::new(&s.id))
        .collect();
    ensure!(
        source_ids == case.sources.iter().map(|p| p.as_path()).collect(),
        "review source parity mismatch"
    );
    for source in &packet.sources {
        ensure!(
            manifest.assets.iter().any(|a| a.kind == AssetKind::Source
                && a.path == std::path::Path::new(&source.id)
                && a.sha256 == digest(source.text.as_bytes())),
            "review source hash mismatch"
        );
    }
    let result = assess_evidence(evidence)?;
    ensure!(
        result.unresolved == observation.unresolved_rubric && (allow_pending || !result.unresolved),
        "unresolved knowledge review"
    );
    ensure!(
        result.success == observation.result.success,
        "knowledge score mismatch"
    );
    Ok(())
}

pub(super) fn critical_failure(observation: &Observation) -> bool {
    observation.critical_failure
        || observation
            .knowledge_review
            .as_ref()
            .is_some_and(|e| assess_evidence(e).map_or(true, |a| a.critical_failure))
}

pub fn assess_evidence(evidence: &KnowledgeEvidence) -> Result<Assessment> {
    ensure!(evidence.history.len() <= 256, "too many review revisions");
    let mut hashes = BTreeSet::new();
    for (index, previous) in evidence.history.iter().enumerate() {
        assess(&evidence.packet, std::slice::from_ref(&previous.ballot))?;
        ensure!(
            hashes.insert(digest_value(&previous.ballot)?),
            "duplicate review revision"
        );
        let next = evidence.history[index + 1..]
            .iter()
            .map(|h| &h.ballot)
            .chain(evidence.ballots.iter())
            .find(|b| b.reviewer == previous.ballot.reviewer)
            .ok_or_else(|| anyhow::anyhow!("review revision lost its successor"))?;
        ensure!(
            next.kind == previous.ballot.kind && digest_value(next)? == previous.replacement_sha256,
            "invalid review revision chain"
        );
    }
    for ballot in &evidence.ballots {
        ensure!(
            hashes.insert(digest_value(ballot)?),
            "review revision cycle"
        );
    }
    assess(&evidence.packet, &evidence.ballots)
}

#[cfg(test)]
mod tests;
