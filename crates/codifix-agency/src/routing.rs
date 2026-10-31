//! Deterministic routing over locally accepted measured profiles, never provider marketing.
use crate::{Mandate, ModelDestination, PolicyError, Usage, valid_identifier};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEvaluation {
    pub evidence_id: String,
    pub workload: String,
    pub evaluated_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub held_out_cases: u32,
    pub quality_ppm: u32,
    /// Total measured cost, including local electricity/compute amortization.
    pub micro_usd_per_call: u64,
    pub latency_ms: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelProfile {
    pub id: String,
    pub model_revision: String,
    pub destination: ModelDestination,
    pub accepted_locally: bool,
    pub context_tokens: u32,
    pub local_memory_mib: u32,
    pub evaluations: Vec<ModelEvaluation>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingRequest {
    pub workload: String,
    pub minimum_quality_ppm: u32,
    pub minimum_held_out_cases: u32,
    pub context_tokens: u32,
    pub max_latency_ms: u32,
    pub available_local_memory_mib: u32,
    /// In addition to mandate disclosure policy, e.g. per-task cloud consent.
    pub permitted_destinations: BTreeSet<ModelDestination>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteDecision {
    pub profile_id: String,
    pub model_revision: String,
    pub destination: ModelDestination,
    pub evaluation_id: String,
    pub reserved_usage: Usage,
}

/// Lower measured all-in cost wins, then latency, then stable profile ID.
/// Authentication, evidence ingestion and the actual reservation are runtime responsibilities.
pub fn select_model(
    profiles: &[ModelProfile],
    request: &RoutingRequest,
    mandate: &Mandate,
    usage: Usage,
    now: DateTime<Utc>,
) -> Result<RouteDecision, PolicyError> {
    mandate.validate(now)?;
    if !valid_identifier(&request.workload)
        || request.minimum_quality_ppm > 1_000_000
        || request.minimum_held_out_cases == 0
        || request.context_tokens == 0
        || request.max_latency_ms == 0
    {
        return Err(PolicyError::Invalid("routing request"));
    }
    let mut ids = BTreeSet::new();
    if profiles.iter().any(|p| !ids.insert(&p.id)) {
        return Err(PolicyError::Invalid("duplicate profile id"));
    }
    let mut eligible = Vec::new();
    for profile in profiles {
        if !profile.accepted_locally
            || !valid_identifier(&profile.id)
            || profile.model_revision.trim().is_empty()
            || profile.destination.validate().is_err()
            || !mandate.destinations.contains(&profile.destination)
            || !request
                .permitted_destinations
                .contains(&profile.destination)
            || profile.context_tokens < request.context_tokens
            || (matches!(profile.destination, ModelDestination::Local { .. })
                && (profile.local_memory_mib == 0
                    || profile.local_memory_mib > request.available_local_memory_mib))
        {
            continue;
        }
        // Select freshest evaluation first so stale good results cannot hide a newer regression.
        let evaluation = profile
            .evaluations
            .iter()
            .filter(|e| e.workload == request.workload && e.evaluated_at <= now)
            .max_by(|a, b| {
                a.evaluated_at
                    .cmp(&b.evaluated_at)
                    .then_with(|| a.evidence_id.cmp(&b.evidence_id))
            });
        let Some(e) = evaluation else {
            continue;
        };
        if !valid_identifier(&e.evidence_id) || e.valid_until <= now || e.valid_until <= e.evaluated_at
            || e.held_out_cases < request.minimum_held_out_cases || e.quality_ppm > 1_000_000
            || e.quality_ppm < request.minimum_quality_ppm || e.latency_ms == 0 || e.latency_ms > request.max_latency_ms
            // Local compute is not represented as a free alternative.
            || (matches!(profile.destination, ModelDestination::Local { .. }) && e.micro_usd_per_call == 0)
        {
            continue;
        }
        let reserve = Usage {
            micro_usd: e.micro_usd_per_call,
            calls: 1,
            turns: 1,
        };
        if usage
            .checked_add(reserve)
            .and_then(|total| mandate.budget.check(total))
            .is_err()
        {
            continue;
        }
        eligible.push((
            e.micro_usd_per_call,
            e.latency_ms,
            &profile.id,
            profile,
            e,
            reserve,
        ));
    }
    eligible.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
    let (_, _, _, profile, e, reserved_usage) =
        eligible.first().ok_or(PolicyError::NoEligibleModel)?;
    Ok(RouteDecision {
        profile_id: profile.id.clone(),
        model_revision: profile.model_revision.clone(),
        destination: profile.destination.clone(),
        evaluation_id: e.evidence_id.clone(),
        reserved_usage: *reserved_usage,
    })
}
