// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, HashSet};

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationReject {
    UnsupportedSchema,
    WrongSpace,
    ForgedPublisher,
    IntegrityMismatch,
    Equivocation,
    StaleOrder,
    Unauthorized,
    NotOrigin,
    CounterConflict,
    CounterExhausted,
    EvidenceScope,
    DuplicateEvidence,
    ProposalClosed,
    StaleProposal,
    AuthorityRevocation,
    InvalidCausation,
    RelationConflict,
    InvalidExpertise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationQuarantine {
    OrderGap,
    MissingDependency,
    VersionGap,
    RevisionGap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", content = "reason", rename_all = "snake_case")]
pub enum PublicationOutcome {
    Applied,
    Duplicate,
    Rejected(PublicationReject),
    Quarantined(PublicationQuarantine),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedVersion {
    pub key: RecordKey,
    pub publication: MemoryPublication,
    pub author: AuthorIdentity,
    pub authored_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalResolution {
    Pending,
    Accepted {
        by: RecordKey,
        version: SharedVersion,
    },
    Rejected {
        by: RecordKey,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProposal {
    pub proposal: CorrectionProposal,
    pub author: AuthorIdentity,
    pub authored_at: DateTime<Utc>,
    pub resolution: ProposalResolution,
}

/// Deterministic reference state for one private space. Runtime authentication,
/// serialized DB acceptance and durable outbox/inbox commits are caller duties.
/// This deliberately retains all accepted history; it is not the storage engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationState {
    space: SpaceId,
    authority: InstanceId,
    sequence: u32,
    memberships: BTreeMap<InstanceId, HashSet<SpaceCapability>>,
    accepted: Vec<PublicationDelivery>,
    by_key: BTreeMap<RecordKey, usize>,
    by_locator: BTreeMap<(InstanceId, RecordLocator), usize>,
    evidence: BTreeMap<RecordKey, ReviewedEvidence>,
    versions: BTreeMap<StreamId, Vec<PublishedVersion>>,
    states: BTreeMap<StreamId, Vec<(RecordKey, StatePublication)>>,
    relations: BTreeMap<RecordKey, Vec<RelationPublication>>,
    proposals: BTreeMap<RecordKey, StoredProposal>,
    expertise: BTreeMap<RecordKey, crate::expertise::CapabilityAdvertisement>,
}

type Transition = Result<(), PublicationOutcome>;
fn reject(reason: PublicationReject) -> Transition {
    Err(PublicationOutcome::Rejected(reason))
}
fn quarantine(reason: PublicationQuarantine) -> Transition {
    Err(PublicationOutcome::Quarantined(reason))
}

impl PublicationState {
    pub fn new(space: SpaceId, authority: InstanceId) -> Self {
        Self {
            space,
            authority,
            sequence: 0,
            memberships: BTreeMap::from([(
                authority,
                HashSet::from([
                    SpaceCapability::Pull,
                    SpaceCapability::Publish,
                    SpaceCapability::ManageMembership,
                ]),
            )]),
            accepted: Vec::new(),
            by_key: BTreeMap::new(),
            by_locator: BTreeMap::new(),
            evidence: BTreeMap::new(),
            versions: BTreeMap::new(),
            states: BTreeMap::new(),
            relations: BTreeMap::new(),
            proposals: BTreeMap::new(),
            expertise: BTreeMap::new(),
        }
    }
    pub fn space(&self) -> SpaceId {
        self.space
    }
    pub fn authority(&self) -> InstanceId {
        self.authority
    }
    pub fn sequence(&self) -> u32 {
        self.sequence
    }
    pub fn has_capability(&self, instance: InstanceId, capability: SpaceCapability) -> bool {
        self.memberships
            .get(&instance)
            .is_some_and(|caps| caps.contains(&capability))
    }
    pub fn accepted(&self) -> &[PublicationDelivery] {
        &self.accepted
    }
    pub fn accepted_record(&self, key: RecordKey) -> Option<&PublicationDelivery> {
        self.accepted.get(*self.by_key.get(&key)?)
    }
    pub fn evidence(&self, key: RecordKey) -> Option<&ReviewedEvidence> {
        self.evidence.get(&key)
    }
    pub fn history(&self, stream: StreamId) -> &[PublishedVersion] {
        self.versions.get(&stream).map_or(&[], Vec::as_slice)
    }
    pub fn state_history(&self, stream: StreamId) -> &[(RecordKey, StatePublication)] {
        self.states.get(&stream).map_or(&[], Vec::as_slice)
    }
    /// Only a state assertion about the current content version is current.
    pub fn current_state(&self, stream: StreamId) -> Option<&StatePublication> {
        let state = &self.state_history(stream).last()?.1;
        (state.content_version.get() == self.content_version(stream)).then_some(state)
    }
    pub fn relation_history(&self, key: RecordKey) -> &[RelationPublication] {
        self.relations.get(&key).map_or(&[], Vec::as_slice)
    }
    pub fn proposal(&self, key: RecordKey) -> Option<&StoredProposal> {
        self.proposals.get(&key)
    }
    pub fn expertise(&self, key: RecordKey) -> Option<&crate::expertise::CapabilityAdvertisement> {
        self.expertise.get(&key)
    }
    pub fn expertise_catalog(
        &self,
    ) -> impl Iterator<Item = &crate::expertise::CapabilityAdvertisement> {
        self.expertise.values()
    }
    pub fn content_version(&self, stream: StreamId) -> u32 {
        self.history(stream)
            .last()
            .map_or(0, |v| v.publication.version.get())
    }
    pub fn state_revision(&self, stream: StreamId) -> u32 {
        self.state_history(stream)
            .last()
            .map_or(0, |(_, s)| s.revision.get())
    }

    /// `authenticated_publisher` must come from verified transport identity,
    /// never the deserialized envelope, author or a local application role.
    pub fn apply(
        &mut self,
        authenticated_publisher: InstanceId,
        delivery: PublicationDelivery,
    ) -> PublicationOutcome {
        let e = &delivery.envelope;
        if authenticated_publisher != e.publisher {
            return PublicationOutcome::Rejected(PublicationReject::ForgedPublisher);
        }
        if !e.supported_schema() {
            return PublicationOutcome::Rejected(PublicationReject::UnsupportedSchema);
        }
        if e.space != self.space {
            return PublicationOutcome::Rejected(PublicationReject::WrongSpace);
        }
        if CanonicalJsonCodec::digest(e).ok().as_ref() != Some(&delivery.digest) {
            return PublicationOutcome::Rejected(PublicationReject::IntegrityMismatch);
        }
        let key = e.key();
        let locator = (e.publisher, delivery.locator.clone());
        if let Some(index) = self
            .by_key
            .get(&key)
            .or_else(|| self.by_locator.get(&locator))
        {
            return if self.accepted.get(*index) == Some(&delivery) {
                PublicationOutcome::Duplicate
            } else {
                PublicationOutcome::Rejected(PublicationReject::Equivocation)
            };
        }
        let Some(next) = self.sequence.checked_add(1) else {
            return PublicationOutcome::Rejected(PublicationReject::CounterExhausted);
        };
        if delivery.sequence.get() > next {
            return PublicationOutcome::Quarantined(PublicationQuarantine::OrderGap);
        }
        if delivery.sequence.get() < next {
            return PublicationOutcome::Rejected(PublicationReject::StaleOrder);
        }
        if e.causation
            .is_some_and(|cause| !self.by_key.contains_key(&cause))
        {
            return PublicationOutcome::Quarantined(PublicationQuarantine::MissingDependency);
        }
        // A failed multi-part transition (including proposal approval) never
        // leaks changes into the caller's projections or acceptance sequence.
        let mut candidate = self.clone();
        if let Err(outcome) = candidate.transition(e) {
            return outcome;
        }
        candidate.sequence = next;
        candidate.by_key.insert(key, candidate.accepted.len());
        candidate
            .by_locator
            .insert(locator, candidate.accepted.len());
        candidate.accepted.push(delivery);
        *self = candidate;
        PublicationOutcome::Applied
    }

    pub fn replay<I>(&mut self, records: I) -> Vec<PublicationOutcome>
    where
        I: IntoIterator<Item = (InstanceId, PublicationDelivery)>,
    {
        records
            .into_iter()
            .map(|(publisher, record)| self.apply(publisher, record))
            .collect()
    }

    fn transition(&mut self, e: &PublicationEnvelope) -> Transition {
        use PublicationRecord::*;
        match &e.record {
            Membership { member, capability } => {
                self.require_authority(e.publisher)?;
                self.memberships
                    .entry(*member)
                    .or_default()
                    .insert(*capability);
            }
            Revocation { member, capability } => {
                self.require_authority(e.publisher)?;
                if *member == self.authority
                    && (capability.is_none()
                        || *capability == Some(SpaceCapability::ManageMembership))
                {
                    return reject(PublicationReject::AuthorityRevocation);
                }
                let caps = self.memberships.entry(*member).or_default();
                match capability {
                    Some(cap) => {
                        caps.remove(cap);
                    }
                    None => caps.clear(),
                }
            }
            _ => {
                if !self.has_capability(e.publisher, SpaceCapability::Publish) {
                    return reject(PublicationReject::Unauthorized);
                }
                self.transition_data(e)?;
            }
        }
        Ok(())
    }

    fn require_authority(&self, publisher: InstanceId) -> Transition {
        if publisher == self.authority
            && self.has_capability(publisher, SpaceCapability::ManageMembership)
        {
            Ok(())
        } else {
            reject(PublicationReject::Unauthorized)
        }
    }

    fn require_origin(publisher: InstanceId, stream: StreamId) -> Transition {
        if publisher == stream.origin {
            Ok(())
        } else {
            reject(PublicationReject::NotOrigin)
        }
    }

    fn check_evidence(&self, stream: StreamId, references: &[RecordKey]) -> Transition {
        let mut seen = HashSet::new();
        for key in references {
            if !seen.insert(key) {
                return reject(PublicationReject::DuplicateEvidence);
            }
            let Some(evidence) = self.evidence.get(key) else {
                return quarantine(PublicationQuarantine::MissingDependency);
            };
            if key.publisher != stream.origin
                || evidence.snapshot().stream != stream
                || evidence.snapshot().space != self.space
            {
                return reject(PublicationReject::EvidenceScope);
            }
        }
        Ok(())
    }

    fn append_version(
        &mut self,
        stream: StreamId,
        publication: MemoryPublication,
        key: RecordKey,
        author: AuthorIdentity,
        authored_at: DateTime<Utc>,
    ) -> Transition {
        Self::require_origin(key.publisher, stream)?;
        check_counter(
            self.content_version(stream),
            publication.version.get(),
            PublicationQuarantine::VersionGap,
        )?;
        self.check_evidence(stream, &publication.evidence)?;
        self.versions
            .entry(stream)
            .or_default()
            .push(PublishedVersion {
                key,
                publication,
                author,
                authored_at,
            });
        Ok(())
    }

    fn transition_data(&mut self, e: &PublicationEnvelope) -> Transition {
        use PublicationRecord::*;
        match &e.record {
            Expertise(ad) => {
                if ad.owner != e.publisher {
                    return reject(PublicationReject::NotOrigin);
                }
                if ad.validate().is_err() {
                    return reject(PublicationReject::InvalidExpertise);
                }
                // Withdrawal is still possible after answering is revoked.
                if ad.active && !self.has_capability(e.publisher, SpaceCapability::AnswerExpertise)
                {
                    return reject(PublicationReject::Unauthorized);
                }
                check_counter(
                    self.expertise(ad.key()).map_or(0, |old| old.revision.get()),
                    ad.revision.get(),
                    PublicationQuarantine::RevisionGap,
                )?;
                self.expertise.insert(ad.key(), ad.clone());
            }
            Evidence(evidence) => {
                let snapshot = evidence.snapshot();
                Self::require_origin(e.publisher, snapshot.stream)?;
                if snapshot.space != self.space {
                    return reject(PublicationReject::EvidenceScope);
                }
                self.evidence.insert(e.key(), evidence.clone());
            }
            MemoryVersion {
                stream,
                publication,
            } => {
                self.append_version(
                    *stream,
                    publication.clone(),
                    e.key(),
                    e.author.clone(),
                    e.authored_at,
                )?;
            }
            CanonicalState { stream, state } => {
                Self::require_origin(e.publisher, *stream)?;
                if self.content_version(*stream) == 0
                    || state.content_version.get() > self.content_version(*stream)
                {
                    return quarantine(PublicationQuarantine::MissingDependency);
                }
                if state.content_version.get() < self.content_version(*stream) {
                    return reject(PublicationReject::CounterConflict);
                }
                check_counter(
                    self.state_revision(*stream),
                    state.revision.get(),
                    PublicationQuarantine::RevisionGap,
                )?;
                self.states
                    .entry(*stream)
                    .or_default()
                    .push((e.key(), state.clone()));
            }
            Relation(relation) => {
                Self::require_origin(e.publisher, relation.source)?;
                if self.content_version(relation.source) == 0
                    || self.content_version(relation.target) == 0
                {
                    return quarantine(PublicationQuarantine::MissingDependency);
                }
                let key = RecordKey {
                    publisher: e.publisher,
                    id: relation.id,
                };
                let previous = self.relation_history(key).last();
                if let Some(previous) = previous {
                    if previous.source != relation.source
                        || previous.target != relation.target
                        || previous.relation_type != relation.relation_type
                    {
                        return reject(PublicationReject::RelationConflict);
                    }
                } else if !relation.active {
                    return quarantine(PublicationQuarantine::MissingDependency);
                }
                check_counter(
                    previous.map_or(0, |r| r.revision.get()),
                    relation.revision.get(),
                    PublicationQuarantine::RevisionGap,
                )?;
                self.relations
                    .entry(key)
                    .or_default()
                    .push(relation.clone());
            }
            Proposal(proposal) => {
                let current = self.content_version(proposal.target);
                if current == 0 || proposal.base_version.get() > current {
                    return quarantine(PublicationQuarantine::MissingDependency);
                }
                if proposal.base_version.get() < current {
                    return reject(PublicationReject::StaleProposal);
                }
                self.check_evidence(proposal.target, &proposal.evidence)?;
                self.proposals.insert(
                    e.key(),
                    StoredProposal {
                        proposal: proposal.clone(),
                        author: e.author.clone(),
                        authored_at: e.authored_at,
                        resolution: ProposalResolution::Pending,
                    },
                );
            }
            ResolveProposal { proposal, source } => {
                let Some(stored) = self.proposals.get(proposal).cloned() else {
                    return quarantine(PublicationQuarantine::MissingDependency);
                };
                Self::require_origin(e.publisher, stored.proposal.target)?;
                if stored.resolution != ProposalResolution::Pending {
                    return reject(PublicationReject::ProposalClosed);
                }
                if e.causation != Some(*proposal) {
                    return reject(PublicationReject::InvalidCausation);
                }
                let resolution = if let Some(source) = source {
                    let current = self.content_version(stored.proposal.target);
                    if current != stored.proposal.base_version.get() {
                        return reject(PublicationReject::StaleProposal);
                    }
                    let Some(next) = current.checked_add(1) else {
                        return reject(PublicationReject::CounterExhausted);
                    };
                    let version = SharedVersion::new(next).map_err(|_| {
                        PublicationOutcome::Rejected(PublicationReject::CounterExhausted)
                    })?;
                    self.append_version(
                        stored.proposal.target,
                        MemoryPublication {
                            version,
                            source: source.clone(),
                            content: stored.proposal.content.clone(),
                            evidence: stored.proposal.evidence.clone(),
                        },
                        e.key(),
                        stored.author.clone(),
                        stored.authored_at,
                    )?;
                    ProposalResolution::Accepted {
                        by: e.key(),
                        version,
                    }
                } else {
                    ProposalResolution::Rejected { by: e.key() }
                };
                self.proposals.insert(
                    *proposal,
                    StoredProposal {
                        resolution,
                        ..stored
                    },
                );
            }
            Membership { .. } | Revocation { .. } => unreachable!("membership routed before data"),
        }
        Ok(())
    }
}

fn check_counter(current: u32, actual: u32, gap: PublicationQuarantine) -> Transition {
    let Some(expected) = current.checked_add(1) else {
        return reject(PublicationReject::CounterExhausted);
    };
    if actual > expected {
        quarantine(gap)
    } else if actual < expected {
        reject(PublicationReject::CounterConflict)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
