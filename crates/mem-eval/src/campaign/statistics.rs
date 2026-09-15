// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comparison {
    pub baseline: Condition,
    pub baseline_success: f64,
    pub delta: f64,
    pub simultaneous_lower: f64,
    pub simultaneous_upper: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionReport {
    pub condition: Condition,
    pub tasks: usize,
    pub successful_executions: usize,
    /// Repeats are executions, never additional independent observations.
    pub total_executions: usize,
    pub critical_failures: usize,
    pub execution_microusd: u64,
    pub total_tokens: u64,
    pub median_duration_ms: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackReport {
    pub track: Track,
    pub independent_clusters: usize,
    pub tasks: usize,
    pub successful_executions: usize,
    pub total_executions: usize,
    pub candidate_success: f64,
    pub comparisons: Vec<Comparison>,
    /// Raw diagnostics for all five conditions, including no-memory.
    #[serde(default)]
    pub conditions: Vec<ConditionReport>,
    pub cost_ratio: Option<f64>,
    pub median_latency_ratio: Option<f64>,
    pub quality_passed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// Execution/accounting/input checks, allowing only genuinely bound pending
    /// semantic judgments. This is not a claim of deployed security or quality.
    #[serde(default)]
    pub execution_valid: bool,
    #[serde(default)]
    pub pending_knowledge_reviews: usize,
    pub harness_valid: bool,
    pub official_evidence: bool,
    pub quality_passed: bool,
    pub errors: Vec<String>,
    pub tracks: Vec<TrackReport>,
    pub execution_microusd: u64,
    pub preparation_microusd: Option<u64>,
    pub preparation_tokens: Option<u64>,
    pub preparation_requests: Option<u64>,
    pub uncertain_attempts: usize,
}

/// Repeated observations are averaged within tasks, then resampled by whole
/// scenario/repository clusters. Max absolute centered deviation constructs a
/// simultaneous 95% bootstrap band for the three paired baseline comparisons.
/// This is an approximate bootstrap interval, not an exact small-sample test.
pub(super) fn simultaneous_interval(clusters: &[[f64; 3]], seed: u64) -> ([f64; 3], f64) {
    let count = clusters.len();
    let mut mean = [0.0; 3];
    for cluster in clusters {
        for i in 0..3 {
            mean[i] += cluster[i] / count as f64;
        }
    }
    let mut rng = seed;
    let mut maxima = Vec::with_capacity(10_000);
    for _ in 0..10_000 {
        let mut resampled = [0.0; 3];
        for _ in 0..count {
            // SplitMix64: specified integer operations, reproducible everywhere.
            rng = rng.wrapping_add(0x9e3779b97f4a7c15);
            let mut x = rng;
            x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
            x ^= x >> 31;
            let cluster = &clusters[(x % count as u64) as usize];
            for i in 0..3 {
                resampled[i] += cluster[i] / count as f64;
            }
        }
        maxima.push(
            (0..3)
                .map(|i| (resampled[i] - mean[i]).abs())
                .fold(0.0, f64::max),
        );
    }
    maxima.sort_by(f64::total_cmp);
    (mean, maxima[9_499])
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}
fn ratio(a: f64, b: f64) -> Option<f64> {
    (b > 0.0).then_some(a / b)
}

pub fn report(manifest: &Manifest, artifact: &Artifact) -> Result<Report> {
    let mut result = report_inner(manifest, artifact, false)?;
    if !result.harness_valid && result.pending_knowledge_reviews > 0 {
        // Use the same fail-closed review validator, not a filter over error
        // strings. Missing usage, forged evidence and isolation failures still
        // invalidate execution even when some judgments are pending.
        result.execution_valid = report_inner(manifest, artifact, true)?.harness_valid;
    }
    Ok(result)
}

pub(super) fn report_for_review(manifest: &Manifest, artifact: &Artifact) -> Result<Report> {
    report_inner(manifest, artifact, true)
}

fn report_inner(manifest: &Manifest, artifact: &Artifact, allow_pending: bool) -> Result<Report> {
    let plan = manifest.plan(artifact.split)?;
    let mut errors = Vec::new();
    if !artifact.offline && manifest.provider_budget.is_none() {
        errors.push("official evidence requires a locked per-call mediation profile".into());
    }
    if !artifact.offline && manifest.preparation_budget.is_none() {
        errors.push("official evidence requires a locked preparation budget and receipt".into());
    }
    if artifact.manifest_sha256 != plan.manifest_sha256 {
        errors.push("manifest hash mismatch".into());
    }
    if !artifact.completed {
        errors.push("campaign interrupted/incomplete".into());
    }
    if artifact.preparation_microusd.is_none() {
        errors.push("unknown preparation costs".into());
    }
    let mut seen = BTreeSet::new();
    let expected: BTreeSet<_> = plan
        .work
        .iter()
        .map(|w| (&w.case_id, w.condition, w.repeat))
        .collect();
    let mut configuration = None;
    for observation in &artifact.observations {
        if !artifact.offline && observation.provider_requests == 0 {
            errors.push("official task has no accounted model invocation".into());
        }
        if let Err(error) = scoring::validate_for_review_with_hash(
            manifest,
            &plan.manifest_sha256,
            observation,
            !artifact.offline,
            allow_pending,
        ) {
            errors.push(error.to_string());
        }
        if observation.model != manifest.model {
            errors.push("provider/model/reasoning/budget mismatch".into());
        }
        if let Some(timing) = &observation.timing {
            if let Err(error) = timing.validate(observation.result.duration_ms) {
                errors.push(error.to_string());
            }
        } else if !artifact.offline {
            errors.push("missing execution-only task timing".into());
        }
        let w = &observation.work;
        let key = (&w.case_id, w.condition, w.repeat);
        if !expected.contains(&key) || !seen.insert(key) {
            errors.push("unexpected/duplicate observation".into());
        }
        if let Some(index) = plan.work.iter().position(|work| work == w)
            && let Err(error) = observation.validate_provider_audit(
                manifest,
                &plan.manifest_sha256,
                &format!("work-{index:05}"),
                artifact.offline,
            )
        {
            errors.push(error.to_string());
        }
        if observation.result.item_id != w.case_id
            || observation.result.skipped
            || observation.result.duration_ms.is_none()
            || observation.result.token_usage.is_none()
            || observation.execution_microusd.is_none()
            || (observation.unresolved_rubric && !allow_pending)
        {
            errors.push(format!(
                "missing or unresolved task evidence: {}",
                w.case_id
            ));
        }
        if let Some(usage) = &observation.result.token_usage
            && usage
                .input_tokens
                .checked_add(usage.output_tokens)
                .is_none_or(|v| v > usage.total_tokens)
        {
            errors.push("inconsistent token accounting".into());
        }
        if let Err(e) = observation.isolation.validate(manifest) {
            errors.push(e.to_string());
        }
        let identity = (
            &observation.isolation.configuration_sha256,
            &observation.isolation.driver_sha256,
            &observation.isolation.service_build,
        );
        if configuration.is_some_and(|previous| previous != identity) {
            errors.push("mixed configuration/driver/build".into());
        }
        configuration = Some(identity);
    }
    if seen != expected {
        errors.push("missing expected observations".into());
    }
    let execution_microusd = artifact
        .observations
        .iter()
        .try_fold(0u64, |sum, o| {
            sum.checked_add(o.execution_microusd.unwrap_or(0))
        })
        .context("cost overflow")?;
    let tokens = artifact
        .observations
        .iter()
        .try_fold(0u64, |sum, o| {
            sum.checked_add(o.result.token_usage.as_ref().map_or(0, |u| u.total_tokens))
        })
        .context("token overflow")?;
    let requests = artifact
        .observations
        .iter()
        .try_fold(0u64, |sum, o| sum.checked_add(o.provider_requests))
        .context("request overflow")?;
    let mut preparation_tokens = 0;
    let mut preparation_requests = 0;
    match (&manifest.preparation_budget, &artifact.preparation_receipt) {
        (Some(limits), Some(receipt)) => {
            if let Err(error) =
                receipt.execution_limits(&manifest.limits, limits, &artifact.manifest_sha256)
            {
                errors.push(error.to_string());
            }
            preparation_tokens = receipt.audit.ledger.tokens;
            preparation_requests = receipt.audit.ledger.requests;
            if artifact.preparation_microusd != Some(receipt.audit.ledger.microusd)
                || artifact
                    .observations
                    .iter()
                    .any(|o| o.isolation.snapshot_sha256 != receipt.snapshot_sha256)
            {
                errors.push("preparation cost or snapshot mismatch".into());
            }
        }
        (None, None) => {}
        _ => errors.push("missing or unexpected preparation receipt".into()),
    }
    if execution_microusd
        .checked_add(artifact.preparation_microusd.unwrap_or(0))
        .is_none_or(|n| n > manifest.limits.microusd)
        || tokens
            .checked_add(preparation_tokens)
            .is_none_or(|n| n > manifest.limits.tokens)
        || requests
            .checked_add(preparation_requests)
            .is_none_or(|n| n > manifest.limits.requests)
    {
        errors.push("campaign limits exceeded".into());
    }
    let uncertain_attempts = artifact
        .observations
        .iter()
        .filter(|o| o.uncertain_charge)
        .count();
    if uncertain_attempts > 0 {
        errors.push("uncertain provider charges require reconciliation".into());
    }
    let harness_valid = errors.is_empty();
    let official_evidence = harness_valid
        && !artifact.observations.iter().any(|o| o.unresolved_rubric)
        && !artifact.offline
        && artifact.split == Split::HeldOut
        && manifest
            .authorize(artifact.split, &artifact.approvals)
            .is_ok();
    let mut tracks = Vec::new();
    if harness_valid {
        for track in [Track::Knowledge, Track::Coding] {
            let cases: Vec<_> = manifest
                .cases
                .iter()
                .filter(|c| c.track == track && c.split == artifact.split)
                .collect();
            let rows: Vec<_> = artifact
                .observations
                .iter()
                .filter(|o| cases.iter().any(|c| c.id == o.work.case_id))
                .collect();
            let candidate: Vec<_> = rows
                .iter()
                .copied()
                .filter(|o| o.work.condition == Condition::FullMemory)
                .collect();
            let successes = candidate.iter().filter(|o| o.result.success).count();
            let candidate_success = successes as f64 / candidate.len() as f64;
            let mut clusters: BTreeMap<&str, Vec<[f64; 3]>> = BTreeMap::new();
            let mut baseline_rates = [0.0; 3];
            for case in &cases {
                let rate = |condition| {
                    rows.iter()
                        .filter(|o| o.work.case_id == case.id && o.work.condition == condition)
                        .filter(|o| o.result.success)
                        .count() as f64
                        / manifest.repeats as f64
                };
                let candidate_rate = rate(Condition::FullMemory);
                let mut deltas = [0.0; 3];
                for (i, baseline) in BASELINES.iter().enumerate() {
                    let baseline_rate = rate(*baseline);
                    baseline_rates[i] += baseline_rate / cases.len() as f64;
                    deltas[i] = candidate_rate - baseline_rate;
                }
                clusters.entry(&case.cluster).or_default().push(deltas);
            }
            let cluster_deltas: Vec<_> = clusters
                .values()
                .map(|tasks| {
                    let mut mean = [0.0; 3];
                    for task in tasks {
                        for i in 0..3 {
                            mean[i] += task[i] / tasks.len() as f64;
                        }
                    }
                    mean
                })
                .collect();
            let (deltas, radius) = simultaneous_interval(&cluster_deltas, manifest.seed);
            let comparisons: Vec<_> = BASELINES
                .iter()
                .enumerate()
                .map(|(i, b)| Comparison {
                    baseline: *b,
                    baseline_success: baseline_rates[i],
                    delta: deltas[i].clamp(-1.0, 1.0),
                    simultaneous_lower: (deltas[i] - radius).clamp(-1.0, 1.0),
                    simultaneous_upper: (deltas[i] + radius).clamp(-1.0, 1.0),
                })
                .collect();
            let reference: Vec<_> = rows
                .iter()
                .copied()
                .filter(|o| Some(o.work.condition) == manifest.reference_baseline)
                .collect();
            let (cost_ratio, median_latency_ratio) = if reference.is_empty() {
                (None, None)
            } else {
                let cost = |items: &[&Observation]| {
                    items
                        .iter()
                        .map(|o| o.execution_microusd.unwrap_or(0) as f64)
                        .sum::<f64>()
                };
                let latency = |items: &[&Observation]| {
                    median(
                        items
                            .iter()
                            .map(|o| o.result.duration_ms.unwrap_or(0) as f64)
                            .collect(),
                    )
                };
                (
                    ratio(cost(&candidate), cost(&reference)),
                    ratio(latency(&candidate), latency(&reference)),
                )
            };
            let floor = if track == Track::Knowledge { 0.9 } else { 0.8 };
            let conditions = CONDITIONS
                .into_iter()
                .map(|condition| {
                    let executions: Vec<_> = rows
                        .iter()
                        .copied()
                        .filter(|o| o.work.condition == condition)
                        .collect();
                    // Campaign totals were checked for overflow above; these are
                    // disjoint nonnegative subsets of those validated parent totals.
                    ConditionReport {
                        condition,
                        tasks: cases.len(),
                        successful_executions: executions
                            .iter()
                            .filter(|o| o.result.success)
                            .count(),
                        total_executions: executions.len(),
                        critical_failures: executions
                            .iter()
                            .filter(|o| scoring::critical_failure(o))
                            .count(),
                        execution_microusd: executions
                            .iter()
                            .map(|o| o.execution_microusd.unwrap_or(0))
                            .sum(),
                        total_tokens: executions
                            .iter()
                            .map(|o| o.result.token_usage.as_ref().map_or(0, |u| u.total_tokens))
                            .sum(),
                        median_duration_ms: median(
                            executions
                                .iter()
                                .map(|o| o.result.duration_ms.unwrap_or(0) as f64)
                                .collect(),
                        ),
                    }
                })
                .collect();
            let quality_passed = official_evidence
                && candidate_success >= floor
                && comparisons
                    .iter()
                    .all(|c| c.delta >= 0.05 && c.simultaneous_lower > 0.0)
                && !rows.iter().any(|o| scoring::critical_failure(o))
                && cost_ratio.is_some_and(|r| r <= 2.0)
                && median_latency_ratio.is_some_and(|r| r <= 2.0);
            tracks.push(TrackReport {
                track,
                independent_clusters: clusters.len(),
                tasks: cases.len(),
                successful_executions: successes,
                total_executions: candidate.len(),
                candidate_success,
                comparisons,
                conditions,
                cost_ratio,
                median_latency_ratio,
                quality_passed,
            });
        }
    }
    errors.sort();
    errors.dedup();
    Ok(Report {
        execution_valid: harness_valid,
        pending_knowledge_reviews: artifact
            .observations
            .iter()
            .filter(|observation| {
                observation.unresolved_rubric && observation.knowledge_review.is_some()
            })
            .count(),
        harness_valid,
        official_evidence,
        quality_passed: official_evidence && tracks.iter().all(|t| t.quality_passed),
        errors,
        tracks,
        execution_microusd,
        preparation_microusd: artifact.preparation_microusd,
        preparation_tokens: artifact
            .preparation_receipt
            .as_ref()
            .map(|r| r.audit.ledger.tokens),
        preparation_requests: artifact
            .preparation_receipt
            .as_ref()
            .map(|r| r.audit.ledger.requests),
        uncertain_attempts,
    })
}
