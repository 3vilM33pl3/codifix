use crate::{PolicyError, valid_identifier};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

/// Opaque exact resource identifiers; no wildcard, path-prefix or implicit inheritance.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceScope {
    pub kind: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelDestination {
    Local { runtime: String },
    Cloud { provider: String, region: String },
}

impl ModelDestination {
    pub fn validate(&self) -> Result<(), PolicyError> {
        let valid = match self {
            Self::Local { runtime } => valid_identifier(runtime),
            Self::Cloud { provider, region } => {
                valid_identifier(provider) && valid_identifier(region)
            }
        };
        valid
            .then_some(())
            .ok_or(PolicyError::Invalid("model destination"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MandateStatus {
    Active,
    Paused,
    Revoked,
}

/// Limits are inclusive and finite. Delegation depth is remaining depth, not usage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub micro_usd: u64,
    pub calls: u32,
    pub turns: u32,
    pub delegation_depth: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    pub micro_usd: u64,
    pub calls: u32,
    pub turns: u32,
}

impl Budget {
    pub fn check(&self, usage: Usage) -> Result<(), PolicyError> {
        if usage.micro_usd > self.micro_usd {
            return Err(PolicyError::Budget("micro_usd"));
        }
        if usage.calls > self.calls {
            return Err(PolicyError::Budget("calls"));
        }
        if usage.turns > self.turns {
            return Err(PolicyError::Budget("turns"));
        }
        Ok(())
    }

    pub fn remaining(&self, usage: Usage) -> Result<Self, PolicyError> {
        self.check(usage)?;
        Ok(Self {
            micro_usd: self.micro_usd - usage.micro_usd,
            calls: self.calls - usage.calls,
            turns: self.turns - usage.turns,
            delegation_depth: self.delegation_depth,
        })
    }
}

impl Usage {
    pub fn checked_add(self, next: Self) -> Result<Self, PolicyError> {
        Ok(Self {
            micro_usd: self
                .micro_usd
                .checked_add(next.micro_usd)
                .ok_or(PolicyError::Budget("micro_usd overflow"))?,
            calls: self
                .calls
                .checked_add(next.calls)
                .ok_or(PolicyError::Budget("calls overflow"))?,
            turns: self
                .turns
                .checked_add(next.turns)
                .ok_or(PolicyError::Budget("turns overflow"))?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mandate {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub issuer_instance_id: Uuid,
    pub subject_instance_id: Uuid,
    pub specialist_id: String,
    pub capabilities: BTreeSet<String>,
    pub resources: BTreeSet<ResourceScope>,
    pub destinations: BTreeSet<ModelDestination>,
    pub expires_at: DateTime<Utc>,
    pub status: MandateStatus,
    pub budget: Budget,
}

impl Mandate {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), PolicyError> {
        if self.id.is_nil()
            || self.issuer_instance_id.is_nil()
            || self.subject_instance_id.is_nil()
            || !valid_identifier(&self.specialist_id)
            || self.parent_id == Some(self.id)
            || self.parent_id.is_some_and(|id| id.is_nil())
            || self.capabilities.is_empty()
            || self.capabilities.iter().any(|c| !valid_identifier(c))
            || self
                .resources
                .iter()
                .any(|r| !valid_identifier(&r.kind) || r.id.trim().is_empty() || r.id.len() > 1024)
        {
            return Err(PolicyError::Invalid("mandate"));
        }
        for destination in &self.destinations {
            destination.validate()?;
        }
        match self.status {
            MandateStatus::Paused => return Err(PolicyError::Inactive("paused")),
            MandateStatus::Revoked => return Err(PolicyError::Inactive("revoked")),
            MandateStatus::Active => {}
        }
        if now >= self.expires_at {
            return Err(PolicyError::Inactive("expired"));
        }
        Ok(())
    }

    /// Checks attenuation against the parent's *remaining* budget. The caller
    /// must reserve that budget and validate the entire live ancestor chain.
    pub fn validate_child(
        &self,
        child: &Self,
        parent_usage: Usage,
        now: DateTime<Utc>,
    ) -> Result<(), PolicyError> {
        self.validate(now)?;
        child.validate(now)?;
        if child.parent_id != Some(self.id)
            || child.id == self.id
            || child.issuer_instance_id != self.subject_instance_id
        {
            return Err(PolicyError::Scope("delegation identity"));
        }
        if child.expires_at > self.expires_at {
            return Err(PolicyError::Scope("expiry"));
        }
        if !child.capabilities.is_subset(&self.capabilities) {
            return Err(PolicyError::Scope("capabilities"));
        }
        if !child.resources.is_subset(&self.resources) {
            return Err(PolicyError::Scope("resources"));
        }
        if !child.destinations.is_subset(&self.destinations) {
            return Err(PolicyError::Scope("destinations"));
        }
        let remaining = self.budget.remaining(parent_usage)?;
        remaining.check(Usage {
            micro_usd: child.budget.micro_usd,
            calls: child.budget.calls,
            turns: child.budget.turns,
        })?;
        if remaining.delegation_depth == 0
            || child.budget.delegation_depth >= remaining.delegation_depth
        {
            return Err(PolicyError::Budget("delegation_depth"));
        }
        Ok(())
    }

    pub fn authorize(
        &self,
        capability: &str,
        resource: &ResourceScope,
        destination: Option<&ModelDestination>,
        cumulative_usage: Usage,
        now: DateTime<Utc>,
    ) -> Result<(), PolicyError> {
        self.validate(now)?;
        if !self.capabilities.contains(capability) {
            return Err(PolicyError::Scope("capability"));
        }
        if !self.resources.contains(resource) {
            return Err(PolicyError::Scope("resource"));
        }
        if destination.is_some_and(|d| !self.destinations.contains(d)) {
            return Err(PolicyError::Scope("destination"));
        }
        self.budget.check(cumulative_usage)
    }

    pub fn pause(&mut self) -> Result<(), PolicyError> {
        if self.status != MandateStatus::Active {
            return Err(PolicyError::Transition);
        }
        self.status = MandateStatus::Paused;
        Ok(())
    }
    pub fn resume(&mut self, now: DateTime<Utc>) -> Result<(), PolicyError> {
        if self.status != MandateStatus::Paused || self.expires_at <= now {
            return Err(PolicyError::Transition);
        }
        self.status = MandateStatus::Active;
        Ok(())
    }
    pub fn revoke(&mut self) {
        self.status = MandateStatus::Revoked;
    }
}
