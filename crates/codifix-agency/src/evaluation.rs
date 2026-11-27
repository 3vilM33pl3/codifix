//! Matched held-out outcome accounting. Inputs are verifier-produced receipts,
//! not self-reported model success. This module checks bindings and arithmetic;
//! it does not fetch evidence or certify the underlying measurements.
use crate::{PolicyError, valid_identifier};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationCondition {
    Deterministic,
    SingleAgent,
    SpecialistNetwork,
}
impl EvaluationCondition {
    pub const ALL: [Self; 3] = [
        Self::Deterministic,
        Self::SingleAgent,
        Self::SpecialistNetwork,
    ];
}
/// Conditions may intentionally use different models/procedures. Keeping both
/// revisions in the plan/report makes those experimental differences explicit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionRevision {
    pub model_revision: String,
    pub procedure_revision: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationPlan {
    pub evaluation_id: String,
    pub dataset_revision: String,
    pub task_revision: String,
    pub verifier_revision: String,
    pub held_out_set_id: String,
    pub training_set_id: String,
    /// Exact case ID -> immutable case revision.
    pub cases: BTreeMap<String, String>,
    pub conditions: BTreeMap<EvaluationCondition, ConditionRevision>,
    /// Synthetic measurements remain visibly labeled in every report.
    pub synthetic: bool,
}
/// A trusted local verifier must produce these fields. A nonempty reference
/// alone does not qualify as verification; the digest binds its exact receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedEvidence {
    pub receipt_ref: String,
    pub receipt_sha256: String,
    pub verifier_revision: String,
    pub verified_success: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeRecord {
    pub evaluation_id: String,
    pub task_id: Uuid,
    pub condition: EvaluationCondition,
    pub dataset_revision: String,
    pub task_revision: String,
    pub case_id: String,
    pub case_revision: String,
    pub model_revision: String,
    pub procedure_revision: String,
    pub held_out_set_id: String,
    pub success: bool,
    pub evidence: VerifiedEvidence,
    /// All-in cost; local compute is included, not added again.
    pub total_micro_usd: u64,
    pub local_compute_micro_usd: u64,
    pub latency_ms: u64,
    pub human_interventions: u32,
    pub synthetic: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionMetrics {
    pub cases: u64,
    pub verified_successes: u64,
    pub total_micro_usd: u64,
    pub local_compute_micro_usd: u64,
    pub total_latency_ms: u64,
    pub human_interventions: u64,
}
/// Differences are comparison minus baseline on exactly the same cases.
/// Signed decimal strings keep the full u64 delta range portable through JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairedComparison {
    pub baseline: EvaluationCondition,
    pub comparison: EvaluationCondition,
    pub matched_cases: u64,
    pub success_delta: String,
    pub total_micro_usd_delta: String,
    pub local_compute_micro_usd_delta: String,
    pub total_latency_ms_delta: String,
    pub human_interventions_delta: String,
    pub baseline_only_successes: u64,
    pub comparison_only_successes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationReport {
    pub plan: EvaluationPlan,
    pub conditions: BTreeMap<EvaluationCondition, ConditionMetrics>,
    pub comparisons: Vec<PairedComparison>,
    /// Deliberately fixed: observations are not a causal/statistical victory claim.
    pub interpretation: String,
}
fn bound_revision(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}
impl EvaluationPlan {
    pub fn validate(&self) -> Result<(), PolicyError> {
        if !valid_identifier(&self.evaluation_id)
            || !bound_revision(&self.dataset_revision)
            || !bound_revision(&self.task_revision)
            || !bound_revision(&self.verifier_revision)
            || !valid_identifier(&self.held_out_set_id)
            || !valid_identifier(&self.training_set_id)
            || self.held_out_set_id == self.training_set_id
            || self.cases.is_empty()
            || self
                .cases
                .iter()
                .any(|(id, revision)| !valid_identifier(id) || !bound_revision(revision))
            || self.conditions.len() != EvaluationCondition::ALL.len()
            || EvaluationCondition::ALL
                .iter()
                .any(|condition| !self.conditions.contains_key(condition))
            || self.conditions.values().any(|r| {
                !bound_revision(&r.model_revision) || !bound_revision(&r.procedure_revision)
            })
        {
            return Err(PolicyError::Invalid("evaluation plan"));
        }
        Ok(())
    }
}
impl VerifiedEvidence {
    fn validate(&self) -> Result<(), PolicyError> {
        if self.receipt_ref.trim().is_empty()
            || self.receipt_ref.len() > 2048
            || !bound_revision(&self.verifier_revision)
            || self.receipt_sha256.len() != 64
            || !self
                .receipt_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(PolicyError::Invalid("verified outcome evidence"));
        }
        Ok(())
    }
}
fn add(total: &mut u64, value: u64) -> Result<(), PolicyError> {
    *total = total
        .checked_add(value)
        .ok_or(PolicyError::Invalid("evaluation metric overflow"))?;
    Ok(())
}
/// Requires one verified outcome for every planned (case, condition), including
/// failed cases. Missing runs cannot silently improve quality or lower cost.
pub fn compare_outcomes(
    plan: &EvaluationPlan,
    records: &[OutcomeRecord],
) -> Result<EvaluationReport, PolicyError> {
    plan.validate()?;
    let expected_count = plan
        .cases
        .len()
        .checked_mul(EvaluationCondition::ALL.len())
        .ok_or(PolicyError::Invalid("evaluation size"))?;
    if records.len() != expected_count {
        return Err(PolicyError::Invalid("missing or extra evaluation outcomes"));
    }
    let mut paired = BTreeMap::new();
    let mut tasks = BTreeSet::new();
    let mut receipts = BTreeSet::new();
    let mut receipt_digests = BTreeSet::new();
    let mut metrics: BTreeMap<_, _> = EvaluationCondition::ALL
        .into_iter()
        .map(|c| (c, ConditionMetrics::default()))
        .collect();
    for record in records {
        let revisions = plan
            .conditions
            .get(&record.condition)
            .ok_or(PolicyError::Invalid("evaluation condition"))?;
        record.evidence.validate()?;
        if record.evaluation_id != plan.evaluation_id
            || record.dataset_revision != plan.dataset_revision
            || record.task_revision != plan.task_revision
            || record.held_out_set_id != plan.held_out_set_id
            || plan.cases.get(&record.case_id) != Some(&record.case_revision)
            || record.model_revision != revisions.model_revision
            || record.procedure_revision != revisions.procedure_revision
            || record.synthetic != plan.synthetic
            || record.task_id.is_nil()
            || record.evidence.verifier_revision != plan.verifier_revision
            || record.success != record.evidence.verified_success
            || record.total_micro_usd < record.local_compute_micro_usd
            || record.latency_ms == 0
            || !tasks.insert(record.task_id)
            || !receipts.insert(&record.evidence.receipt_ref)
            || !receipt_digests.insert(&record.evidence.receipt_sha256)
            || paired
                .insert((record.case_id.clone(), record.condition), record)
                .is_some()
        {
            return Err(PolicyError::Invalid("evaluation outcome binding"));
        }
        let m = metrics
            .get_mut(&record.condition)
            .ok_or(PolicyError::Invalid("evaluation condition"))?;
        add(&mut m.cases, 1)?;
        add(&mut m.verified_successes, u64::from(record.success))?;
        add(&mut m.total_micro_usd, record.total_micro_usd)?;
        add(
            &mut m.local_compute_micro_usd,
            record.local_compute_micro_usd,
        )?;
        add(&mut m.total_latency_ms, record.latency_ms)?;
        add(
            &mut m.human_interventions,
            u64::from(record.human_interventions),
        )?;
    }
    let case_count =
        u64::try_from(plan.cases.len()).map_err(|_| PolicyError::Invalid("evaluation size"))?;
    if metrics.values().any(|m| m.cases != case_count) {
        return Err(PolicyError::Invalid("unmatched evaluation cases"));
    }
    let mut comparisons = Vec::new();
    for (baseline, comparison) in [
        (
            EvaluationCondition::Deterministic,
            EvaluationCondition::SingleAgent,
        ),
        (
            EvaluationCondition::Deterministic,
            EvaluationCondition::SpecialistNetwork,
        ),
        (
            EvaluationCondition::SingleAgent,
            EvaluationCondition::SpecialistNetwork,
        ),
    ] {
        let b = &metrics[&baseline];
        let c = &metrics[&comparison];
        let mut baseline_only_successes = 0;
        let mut comparison_only_successes = 0;
        for id in plan.cases.keys() {
            let b = paired
                .get(&(id.clone(), baseline))
                .ok_or(PolicyError::Invalid("unmatched evaluation case"))?;
            let c = paired
                .get(&(id.clone(), comparison))
                .ok_or(PolicyError::Invalid("unmatched evaluation case"))?;
            baseline_only_successes += u64::from(b.success && !c.success);
            comparison_only_successes += u64::from(!b.success && c.success);
        }
        comparisons.push(PairedComparison {
            baseline,
            comparison,
            matched_cases: case_count,
            success_delta: (i128::from(c.verified_successes) - i128::from(b.verified_successes))
                .to_string(),
            total_micro_usd_delta: (i128::from(c.total_micro_usd) - i128::from(b.total_micro_usd))
                .to_string(),
            local_compute_micro_usd_delta: (i128::from(c.local_compute_micro_usd)
                - i128::from(b.local_compute_micro_usd))
            .to_string(),
            total_latency_ms_delta: (i128::from(c.total_latency_ms)
                - i128::from(b.total_latency_ms))
            .to_string(),
            human_interventions_delta: (i128::from(c.human_interventions)
                - i128::from(b.human_interventions))
            .to_string(),
            baseline_only_successes,
            comparison_only_successes,
        });
    }
    Ok(EvaluationReport {
        plan: plan.clone(),
        conditions: metrics,
        comparisons,
        interpretation: if plan.synthetic {
            "Synthetic fixture measurements only; no real model benefit established."
        } else {
            "Matched observed outcomes only; no causal or statistical superiority established."
        }
        .into(),
    })
}
