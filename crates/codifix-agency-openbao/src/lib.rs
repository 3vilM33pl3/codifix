//! Deterministic, secret-free controller; independent local-share custodians.
//! No Codifix service, database, model or cloud credentials are used.
#![cfg(unix)]
pub mod protected;
pub mod service;
pub mod transport;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::PathBuf};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid enrolled configuration")]
    Configuration,
    #[error("protected local file unavailable or unsafe")]
    ProtectedFile,
    #[error("local state is locked by another process")]
    Busy,
    #[error("state already enrolled; explicit operator recovery required")]
    AlreadyEnrolled,
    #[error("remote transport failed; inspect target locally")]
    Transport,
    #[error("target identity or seal configuration changed; human escalation required")]
    TargetChanged,
    #[error("ceremony expired, replayed or invalid; human escalation required")]
    Ceremony,
    #[error("authentication failed")]
    Authentication,
    #[error("ceremony incomplete; human escalation required")]
    Incomplete,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub url: String,
    /// Dedicated enrollment trust anchor, with built-in roots disabled.
    pub ca_pem_file: Option<PathBuf>,
    /// Deliberate isolated synthetic fixture switch. Only literal loopback HTTP.
    #[serde(default)]
    pub fixture_localhost_http: bool,
}
impl Endpoint {
    pub fn validate(&self) -> Result<()> {
        let u = reqwest::Url::parse(&self.url).map_err(|_| Error::Configuration)?;
        if u.username() != ""
            || u.password().is_some()
            || u.query().is_some()
            || u.fragment().is_some()
            || u.path() != "/"
        {
            return Err(Error::Configuration);
        }
        match u.scheme() {
            "https" if self.ca_pem_file.is_some() && !self.fixture_localhost_http => Ok(()),
            "http"
                if self.fixture_localhost_http
                    && matches!(u.host_str(), Some("127.0.0.1" | "[::1]"))
                    && self.ca_pem_file.is_none() =>
            {
                Ok(())
            }
            _ => Err(Error::Configuration),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub installation_id: String,
    /// Operator-controlled root/unseal-key generation, not the data encryption term.
    pub key_generation: u64,
    /// Fresh attestor process boot ID forces escalation on unreviewed target restart.
    pub boot_id: String,
    pub seal_type: String,
    pub shares: u32,
    pub threshold: u32,
    /// Absolute lease; an unattended stale enrollment never authorizes forever.
    pub valid_until: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub endpoint: Endpoint,
    pub identity_endpoint: Endpoint,
    pub expected: Identity,
    /// Scoped identity-reader token; never a Vault token.
    pub attestor_token_file: PathBuf,
}
impl Target {
    pub fn validate(&self, now: u64) -> Result<()> {
        self.endpoint.validate()?;
        self.identity_endpoint.validate()?;
        let i = &self.expected;
        // Shamir is the sole share-based seal implemented here. Auto-unseal recovery
        // keys cannot be substituted for ordinary unseal shares.
        if i.seal_type != "shamir"
            || i.installation_id.is_empty()
            || i.boot_id.is_empty()
            || i.key_generation == 0
            || i.threshold == 0
            || i.shares < i.threshold
            || i.valid_until <= now
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }
    pub fn binding(&self) -> Result<String> {
        let bytes = serde_json::to_vec(&(
            &self.endpoint.url,
            &self.identity_endpoint.url,
            &self.expected,
        ))
        .map_err(|_| Error::Configuration)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub custodian_id: String,
    pub share_id: String,
    pub failure_domain: String,
}
pub fn validate_roster(roster: &[Member], target: &Target) -> Result<()> {
    let mut ids = BTreeSet::new();
    let mut shares = BTreeSet::new();
    let mut domains = BTreeSet::new();
    for m in roster {
        if m.custodian_id.is_empty()
            || m.share_id.is_empty()
            || m.failure_domain.is_empty()
            || !ids.insert(&m.custodian_id)
            || !shares.insert(&m.share_id)
            || !domains.insert(&m.failure_domain)
        {
            return Err(Error::Configuration);
        }
    }
    if roster.len() < target.expected.threshold as usize
        || roster.len() > target.expected.shares as usize
    {
        return Err(Error::Configuration);
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ceremony {
    pub challenge: String,
    pub sequence: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub target_binding: String,
    pub selected: Vec<String>,
}
impl Ceremony {
    pub fn validate(&self, target: &Target, roster: &[Member], now: u64) -> Result<()> {
        target.validate(now)?;
        validate_roster(roster, target)?;
        if uuid::Uuid::parse_str(&self.challenge).is_err()
            || self.sequence == 0
            || self.issued_at > now
            || self.expires_at <= now
            || self.expires_at <= self.issued_at
            || self.expires_at - self.issued_at > 120
            || self.target_binding != target.binding()?
        {
            return Err(Error::Ceremony);
        }
        let set: BTreeSet<_> = self.selected.iter().collect();
        if set.len() != self.selected.len()
            || set.len() != target.expected.threshold as usize
            || set
                .iter()
                .any(|id| !roster.iter().any(|m| &m.custodian_id == *id))
        {
            return Err(Error::Ceremony);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SealStatus {
    #[serde(rename = "type")]
    pub seal_type: String,
    pub initialized: bool,
    pub sealed: bool,
    pub t: u32,
    pub n: u32,
    #[serde(default)]
    pub progress: u32,
    #[serde(default)]
    pub nonce: String,
    #[serde(default)]
    pub migration: bool,
    #[serde(default)]
    pub recovery_seal: bool,
}
impl SealStatus {
    pub fn check(&self, target: &Target) -> Result<()> {
        let expected = &target.expected;
        if !self.initialized
            || self.seal_type != expected.seal_type
            || self.t != expected.threshold
            || self.n != expected.shares
            || self.migration
            || self.recovery_seal
        {
            return Err(Error::TargetChanged);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerState {
    pub sequence: u64,
    pub active: Option<Ceremony>,
    pub binding: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustodianState {
    pub last_sequence: u64,
    pub last_challenge: Option<String>,
    pub binding: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Observation {
    Unsealed,
    CeremonyStarted { sequence: u64, challenge: String },
    CustodianSubmitted { custodian_id: String, sequence: u64 },
    Escalation { code: String },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peer {
    pub custodian_id: String,
    pub endpoint: Endpoint,
    pub controller_token_file: PathBuf,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerConfig {
    pub target: Target,
    pub roster: Vec<Member>,
    pub peers: Vec<Peer>,
    pub state_file: PathBuf,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustodianConfig {
    pub target: Target,
    pub roster: Vec<Member>,
    pub custodian_id: String,
    pub share_file: PathBuf,
    /// Controller capability scoped solely to this custodian.
    pub controller_token_file: PathBuf,
    pub state_file: PathBuf,
}
impl CustodianConfig {
    pub fn validate(&self, now: u64) -> Result<()> {
        self.target.validate(now)?;
        validate_roster(&self.roster, &self.target)?;
        if !self
            .roster
            .iter()
            .any(|m| m.custodian_id == self.custodian_id)
            || self.share_file == self.controller_token_file
            || self.share_file == self.target.attestor_token_file
            || self.share_file == self.state_file
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
