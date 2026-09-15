// SPDX-License-Identifier: AGPL-3.0-or-later
//! Metadata-only audit of the actual journal and bound result files. This is
//! persistence/accounting evidence, not a signature authenticating a provider.
use super::*;
use crate::campaign::{Ledger, Limits};
use std::collections::BTreeMap;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderCall {
    pub task_id: String,
    pub request_id: String,
    pub payload_sha256: String,
    pub result_sha256: String,
    pub allowance: Allowance,
    pub usage: Allowance,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskUsage {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub microusd: u64,
}
impl TaskUsage {
    fn add(&mut self, usage: Allowance) -> Result<()> {
        self.requests = self.requests.checked_add(1).context("request overflow")?;
        self.input_tokens = self
            .input_tokens
            .checked_add(usage.input_tokens)
            .context("input overflow")?;
        self.output_tokens = self
            .output_tokens
            .checked_add(usage.output_tokens)
            .context("output overflow")?;
        self.microusd = self
            .microusd
            .checked_add(usage.microusd)
            .context("cost overflow")?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderAudit {
    pub version: u8,
    pub manifest_sha256: String,
    pub limits: Limits,
    pub task_limits: Limits,
    pub ledger: Ledger,
    pub tasks: BTreeMap<String, TaskUsage>,
    /// One entry per unique durable request, irrespective of cached deliveries.
    pub calls: Vec<ProviderCall>,
}

impl ProviderResults {
    /// Validate every bound receipt before returning a complete metadata report.
    /// Unsettled calls are not dropped; they make the audit incomplete and fail.
    pub fn audit(&self, budget: &mut RequestBudget) -> Result<ProviderAudit> {
        ensure!(
            budget.uncertain_ids().is_empty(),
            "uncertain calls require reconciliation"
        );
        let (limits, task_limits) = budget.contract();
        let limits = limits.clone();
        let task_limits = task_limits.context("task-scoped journal required")?.clone();
        let mut tasks = BTreeMap::<String, TaskUsage>::new();
        let mut calls = Vec::new();
        let mut total = TaskUsage::default();
        let ids: Vec<String> = budget
            .request_ids()
            .into_iter()
            .map(str::to_owned)
            .collect();
        for id in ids {
            // Recovery verifies the original result bytes, request binding and
            // confirmed usage. With no uncertainty it cannot change the journal.
            self.recover(budget, &id)?;
            let saved = budget.saved_request(&id).context("missing saved request")?;
            let task = saved.task_id.context("missing task identity")?;
            let usage = saved.usage.context("missing usage")?;
            tasks.entry(task.to_owned()).or_default().add(usage)?;
            total.add(usage)?;
            calls.push(ProviderCall {
                task_id: task.to_owned(),
                request_id: id.clone(),
                payload_sha256: saved.payload_sha256.to_owned(),
                result_sha256: saved.result_sha256.context("unbound result")?.to_owned(),
                allowance: saved.allowance,
                usage,
            });
        }
        let ledger = budget.ledger();
        ensure!(
            ledger.requests == total.requests
                && ledger.tokens
                    == total
                        .input_tokens
                        .checked_add(total.output_tokens)
                        .context("token overflow")?
                && ledger.microusd == total.microusd,
            "receipt/journal totals disagree"
        );
        Ok(ProviderAudit {
            version: 1,
            manifest_sha256: budget.identity().to_owned(),
            limits,
            task_limits,
            ledger: Ledger {
                requests: ledger.requests,
                tokens: ledger.tokens,
                microusd: ledger.microusd,
            },
            tasks,
            calls,
        })
    }
}
