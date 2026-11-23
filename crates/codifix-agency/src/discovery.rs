//! Public discovery is a fresh allowlist projection of owner-reviewed public text.
//! No mandate, resource, prompt, task, secret reference, or private endpoint is copied.
use crate::{PolicyError, SpecialistManifest, valid_identifier};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

pub const CAPABILITY_COLLECTION: &str = "dev.codifix.agency.v1.capability";
pub const CAPABILITY_LEXICON: &str =
    include_str!("../lexicons/dev.codifix.agency.v1.capability.json");
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicCapabilityProfile {
    #[serde(rename = "$type")]
    pub record_type: String,
    pub schema_version: u32,
    pub instance_id: Uuid,
    pub specialist_id: String,
    pub display_name: String,
    pub summary: String,
    pub capabilities: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expirations: Vec<CapabilityExpiry>,
    pub agent_card_url: String,
    pub published_at: DateTime<Utc>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityExpiry {
    pub capability: String,
    pub expires_at: DateTime<Utc>,
}

/// Supply separately approved public copy, never raw private manifest text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicProfileApproval {
    pub display_name: String,
    pub summary: String,
    pub capabilities: BTreeSet<String>,
    pub agent_card_url: String,
}
impl PublicCapabilityProfile {
    pub fn from_approved(
        manifest: &SpecialistManifest,
        approval: PublicProfileApproval,
        now: DateTime<Utc>,
    ) -> Result<Self, PolicyError> {
        manifest.validate()?;
        if !approval.capabilities.is_subset(&manifest.capabilities) {
            return Err(PolicyError::Scope("public capabilities"));
        }
        let profile = Self {
            record_type: CAPABILITY_COLLECTION.into(),
            schema_version: 1,
            instance_id: manifest.instance_id,
            specialist_id: manifest.specialist_id.clone(),
            display_name: approval.display_name,
            summary: approval.summary,
            capabilities: approval.capabilities,
            agent_card_url: approval.agent_card_url,
            published_at: now,
            expirations: Vec::new(),
        };
        profile.validate()?;
        Ok(profile)
    }
    /// Apply expiry when reading. The publishing companion can be offline.
    pub fn retain_available(&mut self, now: DateTime<Utc>) {
        self.expirations.retain(|entry| {
            if entry.expires_at <= now {
                self.capabilities.remove(&entry.capability);
                false
            } else {
                true
            }
        });
    }
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.record_type != CAPABILITY_COLLECTION
            || self.schema_version != 1
            || self.instance_id.is_nil()
            || !valid_identifier(&self.specialist_id)
            || self.display_name.trim().is_empty()
            || self.display_name.len() > 200
            || self.summary.trim().is_empty()
            || self.summary.len() > 1000
            || self.capabilities.len() > 100
            || self.capabilities.iter().any(|c| !valid_identifier(c))
        {
            return Err(PolicyError::Invalid("public capability profile"));
        }
        let mut seen = BTreeSet::new();
        for entry in &self.expirations {
            if !self.capabilities.contains(&entry.capability) || !seen.insert(&entry.capability) {
                return Err(PolicyError::Invalid("capability expiry"));
            }
        }
        validate_public_url(&self.agent_card_url)
    }
}
/// Syntactic hygiene only: DNS ownership and public-copy review remain local-owner duties.
pub(crate) fn validate_public_url(value: &str) -> Result<(), PolicyError> {
    let uri: http::Uri = value
        .parse()
        .map_err(|_| PolicyError::Invalid("public URL"))?;
    let host = uri.host().ok_or(PolicyError::Invalid("public URL"))?;
    if uri.scheme_str() != Some("https")
        || uri.query().is_some()
        || value.contains('#')
        || uri.authority().is_none_or(|a| a.as_str().contains('@'))
        || !host.contains('.')
        || host.ends_with(".local")
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
        || host.parse::<std::net::IpAddr>().is_ok()
        || host.starts_with('[')
        || value.len() > 2000
    {
        return Err(PolicyError::Invalid("public URL"));
    }
    Ok(())
}
