// SPDX-License-Identifier: AGPL-3.0-or-later
//! Preparation consumes the same campaign request/token/spend allowance as tasks.
//! Receipt hashes bind evidence, not provider authenticity or authorization.
use super::{Limits, provider_results::ProviderAudit, valid_digest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationReceipt {
    pub version: u8,
    pub snapshot_sha256: String,
    pub audit: ProviderAudit,
}

pub fn validate_limits(global: &Limits, preparation: &Limits) -> Result<()> {
    ensure!(
        preparation.requests > 0
            && preparation.requests < global.requests
            && preparation.tokens > 0
            && preparation.tokens < global.tokens
            && preparation.microusd <= global.microusd
            && (1..=3600).contains(&preparation.task_seconds),
        "invalid preparation allocation"
    );
    Ok(())
}

impl PreparationReceipt {
    pub fn execution_limits(
        &self,
        global: &Limits,
        allocated: &Limits,
        manifest_sha256: &str,
    ) -> Result<Limits> {
        validate_limits(global, allocated)?;
        self.audit.validate()?;
        ensure!(
            self.version == 1
                && valid_digest(&self.snapshot_sha256)
                && self.audit.manifest_sha256 == manifest_sha256
                && self.audit.limits == *allocated
                && self.audit.task_limits == *allocated
                && self.audit.tasks.keys().all(|id| id == "preparation"),
            "preparation receipt contract mismatch"
        );
        Ok(Limits {
            requests: global
                .requests
                .checked_sub(self.audit.ledger.requests)
                .context("preparation request overflow")?,
            tokens: global
                .tokens
                .checked_sub(self.audit.ledger.tokens)
                .context("preparation token overflow")?,
            microusd: global
                .microusd
                .checked_sub(self.audit.ledger.microusd)
                .context("preparation cost overflow")?,
            task_seconds: global.task_seconds,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::{
        Ledger,
        budget::Allowance,
        provider_results::{ProviderCall, TaskUsage},
    };

    #[test]
    fn confirmed_preparation_releases_only_verified_usage_from_global_allowance() {
        let global = Limits {
            requests: 10,
            tokens: 1000,
            microusd: 100,
            task_seconds: 60,
        };
        let allocated = Limits {
            requests: 3,
            tokens: 100,
            microusd: 20,
            task_seconds: 30,
        };
        let hash = "a".repeat(64);
        let receipt = PreparationReceipt {
            version: 1,
            snapshot_sha256: "b".repeat(64),
            audit: ProviderAudit {
                version: 1,
                manifest_sha256: hash.clone(),
                limits: allocated.clone(),
                task_limits: allocated.clone(),
                ledger: Ledger {
                    requests: 1,
                    tokens: 4,
                    microusd: 2,
                },
                tasks: [(
                    "preparation".into(),
                    TaskUsage {
                        requests: 1,
                        input_tokens: 4,
                        output_tokens: 0,
                        microusd: 2,
                    },
                )]
                .into(),
                calls: vec![ProviderCall {
                    task_id: "preparation".into(),
                    request_id: "document-1".into(),
                    payload_sha256: "c".repeat(64),
                    result_sha256: "d".repeat(64),
                    allowance: Allowance {
                        input_tokens: 10,
                        output_tokens: 0,
                        microusd: 5,
                    },
                    usage: Allowance {
                        input_tokens: 4,
                        output_tokens: 0,
                        microusd: 2,
                    },
                }],
            },
        };
        assert_eq!(
            receipt
                .execution_limits(&global, &allocated, &hash)
                .unwrap(),
            Limits {
                requests: 9,
                tokens: 996,
                microusd: 98,
                task_seconds: 60
            }
        );
        assert!(
            receipt
                .execution_limits(&global, &allocated, &"f".repeat(64))
                .is_err()
        );
        for mutate in [
            |r: &mut PreparationReceipt| r.audit.ledger.tokens = 0,
            |r: &mut PreparationReceipt| r.audit.calls[0].task_id = "another-task".into(),
            |r: &mut PreparationReceipt| r.snapshot_sha256 = "not-a-digest".into(),
            |r: &mut PreparationReceipt| r.audit.limits.microusd += 1,
            |r: &mut PreparationReceipt| r.audit.calls[0].usage.microusd = 6,
        ] {
            let mut bad = receipt.clone();
            mutate(&mut bad);
            assert!(bad.execution_limits(&global, &allocated, &hash).is_err());
        }
        let mut excessive = allocated.clone();
        excessive.requests = global.requests;
        assert!(validate_limits(&global, &excessive).is_err());
    }
}
