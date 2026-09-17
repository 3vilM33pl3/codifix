//! Maintenance eligibility only. A trusted adapter must verify the signed
//! manifest and bind every claim below to its verified bytes. Imported release
//! prose, these serialized inputs, or an `Eligible` decision are not installer
//! authority. The installer rechecks local evidence under an instance fence.

use serde::{Deserialize, Serialize};

use crate::{Area, StableId, UnixSeconds};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    Amd64,
    Arm64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub version: StableId,
    pub source_revision: StableId,
    pub manifest_digest: StableId,
    pub channel: StableId,
    pub artifacts: Vec<ReleaseArtifact>,
    /// Explicitly compatible installed versions, from the signed manifest.
    pub compatible_from_versions: Vec<StableId>,
    pub minimum_schema: u32,
    pub maximum_schema: u32,
    pub maintenance: bool,
    pub has_migrations: bool,
    pub changes_trust: bool,
    pub changes_permissions: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseArtifact {
    pub architecture: Architecture,
    pub digest: StableId,
}

/// Result supplied by the local cryptographic verifier, never by a remote notice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum LocalSignature {
    Verified {
        manifest_digest: StableId,
        authority: StableId,
    },
    Unverified,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupEvidence {
    pub id: StableId,
    pub area: Area,
    pub completed_at: UnixSeconds,
    pub schema: u32,
    /// Independent restore proof for the backup procedure/artifact, established
    /// locally. A compression or checksum test does not establish recoverability.
    pub restore_verified: bool,
}

/// Wall time already converted using the IANA Europe/London zone by the runtime.
/// The library deliberately does not accept a server-local timezone assumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LondonLocalTime {
    hour: u8,
    minute: u8,
}

impl LondonLocalTime {
    pub fn new(hour: u8, minute: u8) -> Result<Self, InvalidLocalTime> {
        if hour > 23 || minute > 59 {
            return Err(InvalidLocalTime);
        }
        Ok(Self { hour, minute })
    }

    fn in_maintenance_window(self) -> bool {
        self.hour == 2
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid Europe/London wall-clock hour or minute")]
pub struct InvalidLocalTime;

/// Trusted local facts, obtained again immediately before installation. This is
/// intentionally not deserializable as a remote authorization payload.
#[derive(Debug, Clone, Serialize)]
pub struct UpgradeContext {
    pub area: Area,
    pub now: UnixSeconds,
    pub london_time: LondonLocalTime,
    pub architecture: Architecture,
    pub installed_version: StableId,
    pub schema: u32,
    pub enabled_channel: Option<StableId>,
    pub signature: LocalSignature,
    pub backup: Option<BackupEvidence>,
    /// Locally configured positive freshness limit, not a release-supplied value.
    pub maximum_backup_age_seconds: u64,
    pub conflicting_work: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpgradeDecision {
    manifest_digest: StableId,
    blockers: Vec<UpgradeBlocker>,
}

impl UpgradeDecision {
    pub fn eligible(&self) -> bool {
        self.blockers.is_empty()
    }

    pub fn blockers(&self) -> &[UpgradeBlocker] {
        &self.blockers
    }

    pub fn manifest_digest(&self) -> &StableId {
        &self.manifest_digest
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeBlocker {
    AreaRequiresHumanApproval,
    ChannelNotEnabled,
    OutsideMaintenanceWindow,
    SignatureNotVerified,
    ManifestDigestMismatch,
    NotMaintenance,
    MigrationRequiresHumanApproval,
    TrustChangeRequiresHumanApproval,
    PermissionChangeRequiresHumanApproval,
    ArchitectureUnavailable,
    AmbiguousArchitecture,
    IncompatibleVersion,
    IncompatibleSchema,
    BackupMissing,
    BackupWrongArea,
    BackupWrongSchema,
    BackupNotRecoverable,
    BackupNotFresh,
    ConflictingWork,
}

/// Pure determination for the default 02:00 inclusive–03:00 exclusive window.
/// Human-approved daytime rehearsals require a separate future policy path.
pub fn evaluate_upgrade(release: &ReleaseManifest, context: &UpgradeContext) -> UpgradeDecision {
    let mut blockers = Vec::new();
    if context.area != Area::Homelab {
        blockers.push(UpgradeBlocker::AreaRequiresHumanApproval);
    }
    if context.enabled_channel.as_ref() != Some(&release.channel) {
        blockers.push(UpgradeBlocker::ChannelNotEnabled);
    }
    if !context.london_time.in_maintenance_window() {
        blockers.push(UpgradeBlocker::OutsideMaintenanceWindow);
    }
    match &context.signature {
        LocalSignature::Verified {
            manifest_digest, ..
        } if manifest_digest == &release.manifest_digest => {}
        LocalSignature::Verified { .. } => blockers.push(UpgradeBlocker::ManifestDigestMismatch),
        _ => blockers.push(UpgradeBlocker::SignatureNotVerified),
    }
    if !release.maintenance {
        blockers.push(UpgradeBlocker::NotMaintenance);
    }
    if release.has_migrations {
        blockers.push(UpgradeBlocker::MigrationRequiresHumanApproval);
    }
    if release.changes_trust {
        blockers.push(UpgradeBlocker::TrustChangeRequiresHumanApproval);
    }
    if release.changes_permissions {
        blockers.push(UpgradeBlocker::PermissionChangeRequiresHumanApproval);
    }
    match release
        .artifacts
        .iter()
        .filter(|artifact| artifact.architecture == context.architecture)
        .count()
    {
        0 => blockers.push(UpgradeBlocker::ArchitectureUnavailable),
        1 => {}
        _ => blockers.push(UpgradeBlocker::AmbiguousArchitecture),
    }
    if !release
        .compatible_from_versions
        .contains(&context.installed_version)
    {
        blockers.push(UpgradeBlocker::IncompatibleVersion);
    }
    if context.schema < release.minimum_schema
        || context.schema > release.maximum_schema
        || release.minimum_schema > release.maximum_schema
    {
        blockers.push(UpgradeBlocker::IncompatibleSchema);
    }
    match &context.backup {
        None => blockers.push(UpgradeBlocker::BackupMissing),
        Some(backup) => {
            if backup.area != context.area {
                blockers.push(UpgradeBlocker::BackupWrongArea);
            }
            if backup.schema != context.schema {
                blockers.push(UpgradeBlocker::BackupWrongSchema);
            }
            if !backup.restore_verified {
                blockers.push(UpgradeBlocker::BackupNotRecoverable);
            }
            let age = context.now.0.checked_sub(backup.completed_at.0);
            if context.maximum_backup_age_seconds == 0
                || age.is_none_or(|seconds| seconds > context.maximum_backup_age_seconds)
            {
                blockers.push(UpgradeBlocker::BackupNotFresh);
            }
        }
    }
    if context.conflicting_work {
        blockers.push(UpgradeBlocker::ConflictingWork);
    }
    UpgradeDecision {
        manifest_digest: release.manifest_digest.clone(),
        blockers,
    }
}
