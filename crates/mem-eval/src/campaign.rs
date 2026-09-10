// SPDX-License-Identifier: AGPL-3.0-or-later
//! R3 campaign contracts. Provider/container execution belongs to the CLI.
//! Legacy EvalRun remains readable, but cannot by itself prove R3 validity.
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

pub mod baselines;
mod statistics;
pub use statistics::{Comparison, Report, TrackReport, report};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Condition {
    Handover,
    Lexical,
    Vector,
    FullMemory,
    NoMemory,
}
pub const CONDITIONS: [Condition; 5] = [
    Condition::Handover,
    Condition::Lexical,
    Condition::Vector,
    Condition::FullMemory,
    Condition::NoMemory,
];
pub const BASELINES: [Condition; 3] = [Condition::Handover, Condition::Lexical, Condition::Vector];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Track {
    Knowledge,
    Coding,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Split {
    Development,
    HeldOut,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Source,
    Labels,
    Fixture,
    Configuration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub path: PathBuf,
    pub sha256: String,
    pub kind: AssetKind,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub cluster: String,
    pub track: Track,
    pub split: Split,
    /// Existing mem-eval suite directory, run with just this item selected.
    pub suite: PathBuf,
    pub sources: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub provider: String,
    pub resolved_id: String,
    pub reasoning: String,
    pub embedding_id: String,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    /// Integer USD microdollars per million tokens, from the approved price sheet.
    pub input_microusd_per_million: u64,
    pub output_microusd_per_million: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub requests: u64,
    pub tokens: u64,
    pub microusd: u64,
    pub task_seconds: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub name: String,
    pub semantic_time: DateTime<Utc>,
    pub seed: u64,
    pub repeats: usize,
    pub label_status: String,
    pub model: Model,
    pub limits: Limits,
    pub reference_baseline: Option<Condition>,
    pub assets: Vec<Asset>,
    pub cases: Vec<Case>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    pub manifest_sha256: String,
    pub reviewer: String,
    pub approved_at: DateTime<Utc>,
    /// Separate approvals are needed for label freeze, development spend and official spend.
    pub purpose: ApprovalPurpose,
    pub maximum_microusd: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPurpose {
    Labels,
    Development,
    Official,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Work {
    pub case_id: String,
    pub condition: Condition,
    pub repeat: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub manifest_sha256: String,
    pub split: Split,
    pub work: Vec<Work>,
    pub readiness_errors: Vec<String>,
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn digest_value<T: Serialize>(value: &T) -> Result<String> {
    Ok(digest(&serde_json::to_vec(value)?))
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
pub fn relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let manifest: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        manifest.validate()?;
        manifest.verify_assets(path.parent().context("manifest directory")?)?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && identifier(&self.name),
            "unsupported campaign/name"
        );
        ensure!(self.repeats == 3, "R3 requires three repeats");
        ensure!(
            matches!(self.label_status.as_str(), "draft" | "reviewed"),
            "invalid label status"
        );
        ensure!(
            self.reference_baseline
                .is_none_or(|b| BASELINES.contains(&b)),
            "invalid reference baseline"
        );
        ensure!(
            self.model.max_input_tokens > 0 && self.model.max_output_tokens > 0,
            "invalid model token bounds"
        );
        ensure!(
            self.limits.requests > 0
                && self.limits.tokens > 0
                && self.limits.task_seconds > 0
                && self.limits.task_seconds <= 3600,
            "invalid campaign limits"
        );
        ensure!(
            self.assets.len() <= 10000 && self.cases.len() <= 1000,
            "campaign too large"
        );
        let mut assets = BTreeSet::new();
        for asset in &self.assets {
            ensure!(
                relative_path(&asset.path) && valid_digest(&asset.sha256),
                "invalid asset path/hash"
            );
            ensure!(assets.insert(asset.path.clone()), "duplicate asset");
        }
        for kind in [
            AssetKind::Source,
            AssetKind::Labels,
            AssetKind::Fixture,
            AssetKind::Configuration,
        ] {
            ensure!(
                self.assets.iter().any(|a| a.kind == kind),
                "missing {kind:?} assets"
            );
        }
        let mut ids = BTreeSet::new();
        let mut clusters = BTreeMap::new();
        for case in &self.cases {
            ensure!(
                identifier(&case.id) && identifier(&case.cluster) && ids.insert(&case.id),
                "invalid/duplicate case identity"
            );
            ensure!(relative_path(&case.suite), "invalid suite path");
            ensure!(
                !case.sources.is_empty()
                    && case.sources.iter().all(|p| self
                        .assets
                        .iter()
                        .any(|a| a.path == *p && a.kind == AssetKind::Source)),
                "case has missing/non-source assets"
            );
            if let Some(previous) = clusters.insert(&case.cluster, (case.split, case.track)) {
                ensure!(
                    previous == (case.split, case.track),
                    "cluster crosses splits/tracks"
                );
            }
        }
        for (track, split, count, cluster_count) in [
            (Track::Knowledge, Split::Development, 40, 10),
            (Track::Knowledge, Split::HeldOut, 100, 25),
            (Track::Coding, Split::Development, 4, 4),
            (Track::Coding, Split::HeldOut, 12, 12),
        ] {
            let cases: Vec<_> = self
                .cases
                .iter()
                .filter(|c| c.track == track && c.split == split)
                .collect();
            let actual_clusters: BTreeSet<_> = cases.iter().map(|c| &c.cluster).collect();
            ensure!(
                cases.len() == count && actual_clusters.len() == cluster_count,
                "incorrect {track:?}/{split:?} dataset size"
            );
            let per_cluster = if track == Track::Knowledge { 4 } else { 1 };
            ensure!(
                actual_clusters.iter().all(|cluster| cases
                    .iter()
                    .filter(|c| &c.cluster == *cluster)
                    .count()
                    == per_cluster),
                "unequal cluster size"
            );
        }
        Ok(())
    }

    pub fn verify_assets(&self, root: &Path) -> Result<()> {
        let root = root.canonicalize()?;
        for asset in &self.assets {
            let path = root.join(&asset.path).canonicalize()?;
            ensure!(
                path.starts_with(&root) && path.is_file(),
                "asset escapes campaign directory"
            );
            ensure!(
                digest(&std::fs::read(path)?) == asset.sha256,
                "asset hash mismatch: {}",
                asset.path.display()
            );
        }
        let mut suites = BTreeMap::new();
        for case in &self.cases {
            if !suites.contains_key(&case.suite) {
                let directory = root.join(&case.suite).canonicalize()?;
                ensure!(
                    directory.starts_with(&root),
                    "suite escapes campaign directory"
                );
                let suite = crate::load_suite(&directory)?;
                for path in [
                    directory.join("suite.toml"),
                    directory.join(&suite.manifest.items),
                ] {
                    let path = path.canonicalize()?;
                    ensure!(self.assets.iter().any(|a| root.join(&a.path).canonicalize().ok().as_ref() == Some(&path)), "suite content is not hash-locked");
                }
                suites.insert(case.suite.clone(), suite);
            }
            let suite = suites.get(&case.suite).context("loaded suite")?;
            let items: Vec<_> = suite
                .items
                .iter()
                .filter(|item| item.id() == case.id)
                .collect();
            ensure!(
                items.len() == 1,
                "case is missing/duplicated in suite: {}",
                case.id
            );
            match (case.track, items[0]) {
                (Track::Knowledge, crate::EvalItem::GroundedAnswer(_)) => {}
                (Track::Coding, crate::EvalItem::AgentBuildSequence(sequence))
                    if sequence.steps.len() == 3 => {}
                _ => anyhow::bail!("case has wrong task type or step count"),
            }
        }
        Ok(())
    }

    pub fn plan(&self, split: Split) -> Result<Plan> {
        self.validate()?;
        let mut work = Vec::new();
        for case in self.cases.iter().filter(|c| c.split == split) {
            for repeat in 0..self.repeats {
                for condition in CONDITIONS {
                    work.push(Work {
                        case_id: case.id.clone(),
                        condition,
                        repeat,
                    });
                }
            }
        }
        // Hash sorting is deterministic across platforms and Rust versions.
        work.sort_by_cached_key(|w| {
            digest(format!("{}:{}:{:?}:{}", self.seed, w.case_id, w.condition, w.repeat).as_bytes())
        });
        let mut readiness_errors = Vec::new();
        if self.label_status != "reviewed" {
            readiness_errors.push("labels require Olivier's approval".into());
        }
        if self.reference_baseline.is_none() {
            readiness_errors.push("freeze development-selected reference baseline".into());
        }
        if self.model.resolved_id == "pending"
            || self.model.provider == "pending"
            || self.model.embedding_id == "pending"
        {
            readiness_errors.push("resolved provider/model/embedding approval required".into());
        }
        if self.limits.microusd == 0 {
            readiness_errors.push("paid campaign budget not approved".into());
        }
        Ok(Plan {
            manifest_sha256: digest_value(self)?,
            split,
            work,
            readiness_errors,
        })
    }

    pub fn authorize(&self, split: Split, approvals: &[Approval]) -> Result<()> {
        let plan = self.plan(split)?;
        ensure!(
            plan.readiness_errors.is_empty(),
            "campaign not ready: {:?}",
            plan.readiness_errors
        );
        for purpose in [
            ApprovalPurpose::Labels,
            if split == Split::Development {
                ApprovalPurpose::Development
            } else {
                ApprovalPurpose::Official
            },
        ] {
            ensure!(
                approvals.iter().any(|a| a.purpose == purpose
                    && a.manifest_sha256 == plan.manifest_sha256
                    && !a.reviewer.trim().is_empty()
                    && a.approved_at <= Utc::now()
                    && (purpose == ApprovalPurpose::Labels
                        || a.maximum_microusd >= self.limits.microusd)),
                "missing current {purpose:?} approval"
            );
        }
        Ok(())
    }
}

/// An executor attests these checks. A boolean declaration alone is not security
/// evidence: the locked driver and its logs must accompany the campaign archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Isolation {
    pub before_sha256: String,
    pub after_sha256: String,
    pub snapshot_sha256: String,
    pub configuration_sha256: String,
    pub driver_sha256: String,
    pub service_build: String,
    pub semantic_time: DateTime<Utc>,
    pub sealed: bool,
    pub restricted_network: bool,
    pub hidden_tests_inaccessible: bool,
}
impl Isolation {
    pub fn validate(&self, manifest: &Manifest) -> Result<()> {
        ensure!(
            [
                &self.before_sha256,
                &self.after_sha256,
                &self.snapshot_sha256,
                &self.configuration_sha256,
                &self.driver_sha256
            ]
            .into_iter()
            .all(|s| valid_digest(s)),
            "missing isolation hashes"
        );
        ensure!(
            self.before_sha256 == self.after_sha256 && self.before_sha256 == self.snapshot_sha256,
            "benchmark state changed or snapshot reset failed"
        );
        ensure!(
            self.sealed && self.restricted_network && self.hidden_tests_inaccessible,
            "isolation controls failed"
        );
        ensure!(
            self.semantic_time == manifest.semantic_time && !self.service_build.is_empty(),
            "clock/build mismatch"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub work: Work,
    pub model: Model,
    /// Existing task-level result; its children remain diagnostics.
    pub result: crate::EvalItemResult,
    pub execution_microusd: Option<u64>,
    pub provider_requests: u64,
    pub uncertain_charge: bool,
    pub critical_failure: bool,
    pub unresolved_rubric: bool,
    pub isolation: Isolation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub manifest_sha256: String,
    pub split: Split,
    pub offline: bool,
    pub completed: bool,
    pub preparation_microusd: Option<u64>,
    pub approvals: Vec<Approval>,
    pub observations: Vec<Observation>,
}

/// Reserve worst-case spend before dispatch, never after a provider response.
/// A reservation with uncertain outcome stays consumed; only confirmed usage
/// can release its unused portion. Persist this ledger before network I/O.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    pub requests: u64,
    pub tokens: u64,
    pub microusd: u64,
}
impl Ledger {
    pub fn reserve(&mut self, limits: &Limits, tokens: u64, microusd: u64) -> Result<()> {
        let requests = self.requests.checked_add(1).context("request overflow")?;
        let total_tokens = self.tokens.checked_add(tokens).context("token overflow")?;
        let cost = self
            .microusd
            .checked_add(microusd)
            .context("cost overflow")?;
        ensure!(
            requests <= limits.requests && total_tokens <= limits.tokens && cost <= limits.microusd,
            "campaign budget exhausted; retain interrupted artifact"
        );
        self.requests = requests;
        self.tokens = total_tokens;
        self.microusd = cost;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
