use serde::Serialize;

use crate::{StableId, UnixSeconds};

/// Fencing authority for one action. A storage adapter must serialize mutations
/// of this value with compare-and-swap/row locks; cloning it is not a second lock.
#[derive(Debug, Clone, Serialize)]
pub struct LeaseAuthority {
    resource: StableId,
    generation: u64,
    current: Option<Lease>,
}

/// Opaque claim; possession alone is insufficient. Always validate against the
/// current authority at the same atomic boundary as an effect or result write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Lease {
    resource: StableId,
    worker: StableId,
    generation: u64,
    expires_at: UnixSeconds,
}

impl Lease {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn expires_at(&self) -> UnixSeconds {
        self.expires_at
    }
}

impl LeaseAuthority {
    pub fn new(resource: StableId) -> Self {
        Self {
            resource,
            generation: 0,
            current: None,
        }
    }

    pub fn claim(
        &mut self,
        worker: StableId,
        now: UnixSeconds,
        ttl_seconds: u64,
    ) -> Result<Lease, LeaseError> {
        if self
            .current
            .as_ref()
            .is_some_and(|lease| now < lease.expires_at)
        {
            return Err(LeaseError::AlreadyClaimed);
        }
        let expires_at = expiry(now, ttl_seconds)?;
        let generation = self.generation.checked_add(1).ok_or(LeaseError::Overflow)?;
        let lease = Lease {
            resource: self.resource.clone(),
            worker,
            generation,
            expires_at,
        };
        self.generation = generation;
        self.current = Some(lease.clone());
        Ok(lease)
    }

    pub fn validate(&self, lease: &Lease, now: UnixSeconds) -> Result<(), LeaseError> {
        if self.current.as_ref() != Some(lease) || lease.generation != self.generation {
            return Err(LeaseError::StaleFence);
        }
        if now >= lease.expires_at {
            return Err(LeaseError::Expired);
        }
        Ok(())
    }

    pub fn renew(
        &mut self,
        lease: &Lease,
        now: UnixSeconds,
        ttl_seconds: u64,
    ) -> Result<Lease, LeaseError> {
        self.validate(lease, now)?;
        let expires_at = expiry(now, ttl_seconds)?;
        if expires_at <= lease.expires_at {
            return Err(LeaseError::InvalidDuration);
        }
        let updated = Lease {
            expires_at,
            ..lease.clone()
        };
        self.current = Some(updated.clone());
        Ok(updated)
    }

    pub fn revoke(&mut self) -> Result<(), LeaseError> {
        self.generation = self.generation.checked_add(1).ok_or(LeaseError::Overflow)?;
        self.current = None;
        Ok(())
    }

    pub fn is_expired(&self, now: UnixSeconds) -> bool {
        self.current
            .as_ref()
            .is_some_and(|lease| now >= lease.expires_at)
    }
}

fn expiry(now: UnixSeconds, ttl: u64) -> Result<UnixSeconds, LeaseError> {
    if ttl == 0 {
        return Err(LeaseError::InvalidDuration);
    }
    now.0
        .checked_add(ttl)
        .map(UnixSeconds)
        .ok_or(LeaseError::Overflow)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LeaseError {
    #[error("another live claim already owns this action")]
    AlreadyClaimed,
    #[error("lease token is no longer current")]
    StaleFence,
    #[error("lease has expired")]
    Expired,
    #[error("lease duration must advance expiry")]
    InvalidDuration,
    #[error("lease counter or expiry overflow")]
    Overflow,
}
