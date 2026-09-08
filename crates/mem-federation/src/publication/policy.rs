// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local policy intersection; never a conversion from an application role to
//! remote authority. Inputs must be resolved by the service, not request JSON.

use mem_record::{Permission, PermissionSet};

use super::*;

/// Effective permissions for precisely one server-resolved local project.
/// Deliberately not deserializable. Construction does not authenticate a caller.
#[derive(Debug, Clone)]
pub struct LocalProjectAccess {
    pub principal: Uuid,
    pub project: Uuid,
    pub permissions: PermissionSet,
    pub read_only: bool,
}

/// Explicit opt-in maintained locally, independently of gateway enrollment.
/// No role preset or global-admin shortcut creates this grant automatically.
#[derive(Debug, Clone)]
pub struct LocalSpaceGrant {
    pub principal: Uuid,
    pub project: Uuid,
    pub space: SpaceId,
    pub instance: InstanceId,
    pub capabilities: Vec<SpaceCapability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalPolicyReject {
    InvalidLocalIdentity,
    MissingLocalGrant,
    WrongPrincipal,
    WrongProject,
    WrongSpace,
    WrongInstance,
    ReadOnly,
    MissingPermission(Permission),
    MissingLocalCapability(SpaceCapability),
    MissingGatewayCapability(SpaceCapability),
    NotSpaceAuthority,
}

impl PublicationState {
    /// Gate a live shared read/pull. Offline-copy retention is a separate policy,
    /// not permission to continue using a revoked gateway credential.
    pub fn authorize_pull(
        &self,
        local: &LocalProjectAccess,
        grant: Option<&LocalSpaceGrant>,
        instance: InstanceId,
    ) -> Result<(), LocalPolicyReject> {
        self.authorize_local(local, grant, instance, SpaceCapability::Pull, false)?;
        require_permissions(local, &[Permission::MemoryRead])
    }

    /// Gate local publication intent using the actual record, so a caller cannot
    /// label a tombstone or membership change as an ordinary content update.
    /// The gateway must separately authenticate/recheck/apply in its transaction.
    pub fn authorize_publication(
        &self,
        local: &LocalProjectAccess,
        grant: Option<&LocalSpaceGrant>,
        envelope: &PublicationEnvelope,
    ) -> Result<(), LocalPolicyReject> {
        if envelope.space != self.space() {
            return Err(LocalPolicyReject::WrongSpace);
        }
        let membership = matches!(
            envelope.record,
            PublicationRecord::Membership { .. } | PublicationRecord::Revocation { .. }
        );
        let capability = if membership {
            SpaceCapability::ManageMembership
        } else {
            SpaceCapability::Publish
        };
        self.authorize_local(local, grant, envelope.publisher, capability, true)?;
        if membership {
            require_permissions(local, &[Permission::MemoryRead, Permission::AuthManage])?;
            if envelope.publisher != self.authority() {
                return Err(LocalPolicyReject::NotSpaceAuthority);
            }
        } else {
            require_permissions(
                local,
                &[
                    Permission::MemoryRead,
                    Permission::MemoryCurate,
                    Permission::BundleExport,
                ],
            )?;
            if matches!(&envelope.record, PublicationRecord::CanonicalState { state, .. } if state.tombstone)
            {
                require_permissions(local, &[Permission::MemoryDelete])?;
            }
        }
        Ok(())
    }

    fn authorize_local(
        &self,
        local: &LocalProjectAccess,
        grant: Option<&LocalSpaceGrant>,
        instance: InstanceId,
        capability: SpaceCapability,
        write: bool,
    ) -> Result<(), LocalPolicyReject> {
        if local.principal.is_nil() || local.project.is_nil() {
            return Err(LocalPolicyReject::InvalidLocalIdentity);
        }
        let grant = grant.ok_or(LocalPolicyReject::MissingLocalGrant)?;
        if grant.principal != local.principal {
            return Err(LocalPolicyReject::WrongPrincipal);
        }
        if grant.project != local.project {
            return Err(LocalPolicyReject::WrongProject);
        }
        if grant.space != self.space() {
            return Err(LocalPolicyReject::WrongSpace);
        }
        if grant.instance != instance {
            return Err(LocalPolicyReject::WrongInstance);
        }
        if write && local.read_only {
            return Err(LocalPolicyReject::ReadOnly);
        }
        if !grant.capabilities.contains(&capability) {
            return Err(LocalPolicyReject::MissingLocalCapability(capability));
        }
        if !self.has_capability(instance, capability) {
            return Err(LocalPolicyReject::MissingGatewayCapability(capability));
        }
        Ok(())
    }
}

fn require_permissions(
    local: &LocalProjectAccess,
    required: &[Permission],
) -> Result<(), LocalPolicyReject> {
    for permission in required {
        if !local.permissions.contains(*permission) {
            return Err(LocalPolicyReject::MissingPermission(*permission));
        }
    }
    Ok(())
}
