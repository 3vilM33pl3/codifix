// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit authority-side membership operator. No database, provisioning,
//! automatic discovery, credential generation, arbitrary records or delegation.
use crate::security::PageVerificationError;
use crate::sync::PullFailure;
use crate::{Endpoint, GatewayClient, ReceiptTrust, private_files, root_certificate};
use chrono::{DateTime, Utc};
use codifix_federation::{
    AuthorIdentity, CanonicalJsonCodec, ContentDigest, RecordLocator, SpaceCapability,
    publication::*,
};
use codifix_federation_store::{AuditReceipt, PublicationIntent, ReceiptKey, audit_digest};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_HISTORY_BYTES: usize = 16 * 1024 * 1024;
const MAX_HISTORY_RECORDS: usize = 4096;

#[derive(Debug, thiserror::Error)]
pub enum EnrollmentError {
    #[error("invalid private enrollment configuration or reviewed bounds")]
    Configuration,
    #[error("exact enrollment review digest required; no submission authorized")]
    Confirmation,
    #[error("enrollment review expired or is not yet valid; prepare a new reviewed plan")]
    Expired,
    #[error("signed gateway history no longer matches the reviewed baseline")]
    ChangedHistory,
    #[error("gateway proof rejected; no publication claim is established")]
    InvalidProof,
    #[error("gateway transport unavailable; no publication claim is established")]
    GatewayUnavailable,
    #[error("gateway authorization denied; no publication claim is established")]
    GatewayUnauthorized,
    #[error("gateway page could not be decoded; no publication claim is established")]
    InvalidPageEncoding,
    #[error("gateway returned an unexpected status; no publication claim is established")]
    InvalidGatewayStatus,
    #[error("gateway page binding rejected; no publication claim is established")]
    InvalidPageBinding,
    #[error("gateway page policy rejected; no publication claim is established")]
    InvalidPagePolicy,
    #[error("gateway page signature rejected; no publication claim is established")]
    InvalidPageSignature,
    #[error("gateway page receipt rejected; no publication claim is established")]
    InvalidPageReceipt,
    #[error("gateway page receipt chain rejected; no publication claim is established")]
    InvalidPageChain,
    #[error("gateway page order rejected; no publication claim is established")]
    InvalidPageOrder,
    #[error("gateway complete history proof rejected; no publication claim is established")]
    InvalidHistoryProof,
    #[error("gateway published history does not replay; no publication claim is established")]
    InvalidHistoryReplay,
    #[error("gateway outcome unconfirmed; inspect or retry only the exact reviewed plan")]
    Unconfirmed,
    #[error("gateway rejected a reviewed membership; inspect before any new plan")]
    Rejected,
}
type Result<T> = std::result::Result<T, EnrollmentError>;

/// The four independent IDs are part of the frozen confirmation. Never mint
/// replacement identities after a lost reply or manufacture ManageMembership.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberEnrollment {
    pub member: InstanceId,
    pub pull_record: RecordId,
    pub publish_record: RecordId,
    pub ask_record: RecordId,
    pub answer_record: RecordId,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentPlan {
    pub operation_id: Uuid,
    pub space: SpaceId,
    pub authority: InstanceId,
    pub author: AuthorIdentity,
    pub authored_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// Independently verified published baseline, not DB allocation/order.
    pub expected_sequence: u32,
    pub expected_tip: Option<ContentDigest>,
    pub members: Vec<MemberEnrollment>,
}

/// Operator-owned files only. A token is loaded solely by inspect/execute;
/// preview output omits its path and never contains token bytes.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentConfig {
    pub schema_version: u32,
    pub gateway: String,
    pub gateway_ca_file: Option<PathBuf>,
    pub gateway_ca_sha256: Option<ContentDigest>,
    pub token_file: PathBuf,
    pub receipt_keys: Vec<ReceiptKey>,
    pub plan: EnrollmentPlan,
}

#[derive(Serialize)]
pub struct EnrollmentPreview {
    pub status: &'static str,
    pub review_sha256: String,
    pub gateway: String,
    pub receipt_keys: Vec<ReceiptKey>,
    pub gateway_ca_sha256: Option<ContentDigest>,
    pub plan: EnrollmentPlan,
    pub records: Vec<PublicationIntent>,
    pub network_calls: u32,
}

#[derive(Serialize)]
pub struct EnrollmentInspection {
    pub published_through: u32,
    pub published_tip: Option<ContentDigest>,
    pub members: Vec<MemberInspection>,
}

#[derive(Serialize)]
pub struct MemberInspection {
    pub member: InstanceId,
    pub capabilities: Vec<SpaceCapability>,
}

#[derive(Serialize)]
pub struct EnrollmentReport {
    pub review_sha256: String,
    /// Acceptance is not publication. This report derives published records
    /// only from nonce-bound signed pages and the complete verified audit chain.
    pub status: &'static str,
    pub accepted: Vec<AcceptedMembership>,
    pub confirmed_published_records: Vec<RecordId>,
    pub observed: EnrollmentInspection,
}

#[derive(Serialize)]
pub struct AcceptedMembership {
    pub record_id: RecordId,
    pub authority_sequence: u32,
}

struct Snapshot {
    records: Vec<PublicationDelivery>,
    receipts: Vec<AuditReceipt>,
    state: PublicationState,
}

impl EnrollmentConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = private_files::read(path, MAX_CONFIG_BYTES)
            .map_err(|_| EnrollmentError::Configuration)?;
        let config: Self =
            serde_json::from_slice(&bytes).map_err(|_| EnrollmentError::Configuration)?;
        config.validate()?;
        Ok(config)
    }

    fn endpoint(&self) -> Result<Endpoint> {
        let ca = match (&self.gateway_ca_file, &self.gateway_ca_sha256) {
            (None, None) => None,
            (Some(path), Some(expected)) if path.is_absolute() => {
                let bytes = root_certificate(path).map_err(|_| EnrollmentError::Configuration)?;
                if ContentDigest::sha256_bytes(&bytes) != *expected {
                    return Err(EnrollmentError::Configuration);
                }
                Some(bytes)
            }
            _ => return Err(EnrollmentError::Configuration),
        };
        Endpoint::with_ca(&self.gateway, false, ca.as_deref())
            .map_err(|_| EnrollmentError::Configuration)
    }

    fn validate(&self) -> Result<()> {
        let p = &self.plan;
        if self.schema_version != 1
            || !self.token_file.is_absolute()
            || p.operation_id.is_nil()
            || p.members.is_empty()
            || p.members.len() > 8
            || p.expected_sequence as usize > MAX_HISTORY_RECORDS
            || (p.expected_sequence == 0) != p.expected_tip.is_none()
            || p.expires_at <= p.authored_at
            || p.expires_at.signed_duration_since(p.authored_at) > chrono::Duration::hours(24)
        {
            return Err(EnrollmentError::Configuration);
        }
        self.endpoint()?;
        ReceiptTrust::new(self.receipt_keys.clone()).map_err(|_| EnrollmentError::Configuration)?;
        let mut members = BTreeSet::new();
        let mut records = BTreeSet::new();
        for member in &p.members {
            // Separate authority means no implicit enrollment of Workshop as
            // authority and no accidental membership grant to the operator.
            if member.member == p.authority || !members.insert(member.member) {
                return Err(EnrollmentError::Configuration);
            }
            for record in [
                member.pull_record,
                member.publish_record,
                member.ask_record,
                member.answer_record,
            ] {
                if !records.insert(record) {
                    return Err(EnrollmentError::Configuration);
                }
            }
        }
        Ok(())
    }

    fn digest(&self) -> Result<ContentDigest> {
        CanonicalJsonCodec::digest(self).map_err(|_| EnrollmentError::Configuration)
    }

    fn intents(&self) -> Result<Vec<PublicationIntent>> {
        let mut result = Vec::new();
        for member in &self.plan.members {
            for (record_id, capability) in [
                (member.pull_record, SpaceCapability::Pull),
                (member.publish_record, SpaceCapability::Publish),
                (member.ask_record, SpaceCapability::AskExpertise),
                (member.answer_record, SpaceCapability::AnswerExpertise),
            ] {
                result.push(PublicationIntent {
                    locator: RecordLocator::new(format!(
                        "enrollment:{}:{}",
                        self.plan.operation_id,
                        record_id.as_uuid()
                    ))
                    .map_err(|_| EnrollmentError::Configuration)?,
                    envelope: PublicationEnvelope {
                        schema_version: 3,
                        space: self.plan.space,
                        publisher: self.plan.authority,
                        record_id,
                        author: self.plan.author.clone(),
                        authored_at: self.plan.authored_at,
                        causation: None,
                        record: PublicationRecord::Membership {
                            member: member.member,
                            capability,
                        },
                    },
                });
            }
        }
        Ok(result)
    }

    pub fn preview(&self) -> Result<EnrollmentPreview> {
        self.validate()?;
        Ok(EnrollmentPreview {
            status: "review_only_no_network_or_secret_read",
            review_sha256: self.digest()?.as_hex().into(),
            gateway: self.gateway.clone(),
            receipt_keys: self.receipt_keys.clone(),
            gateway_ca_sha256: self.gateway_ca_sha256.clone(),
            plan: self.plan.clone(),
            records: self.intents()?,
            network_calls: 0,
        })
    }

    fn client(&self) -> Result<GatewayClient> {
        self.validate()?;
        let token = String::from_utf8(
            private_files::read(&self.token_file, 4096)
                .map_err(|_| EnrollmentError::Configuration)?,
        )
        .map_err(|_| EnrollmentError::Configuration)?;
        if token.trim().len() < 32 {
            return Err(EnrollmentError::Configuration);
        }
        GatewayClient::new(
            self.endpoint()?,
            token.trim().to_owned(),
            self.plan.space,
            self.plan.authority,
            ReceiptTrust::new(self.receipt_keys.clone())
                .map_err(|_| EnrollmentError::Configuration)?,
        )
        .map_err(|_| EnrollmentError::Configuration)
    }

    pub async fn inspect(&self) -> Result<EnrollmentInspection> {
        self.inspection(&self.snapshot(&self.client()?).await?)
    }

    pub async fn execute(&self, confirmation: &str) -> Result<EnrollmentReport> {
        self.validate()?;
        let digest = self.digest()?;
        if confirmation != digest.as_hex() {
            return Err(EnrollmentError::Confirmation);
        }
        self.current()?;
        let client = self.client()?;
        self.execute_with(&client, digest).await
    }

    fn current(&self) -> Result<()> {
        let now = Utc::now();
        if now < self.plan.authored_at || now >= self.plan.expires_at {
            return Err(EnrollmentError::Expired);
        }
        Ok(())
    }

    async fn execute_with(
        &self,
        client: &GatewayClient,
        digest: ContentDigest,
    ) -> Result<EnrollmentReport> {
        let intents = self.intents()?;
        self.check_baseline(&self.snapshot(client).await?, &intents)?;
        let mut accepted = Vec::new();
        for intent in &intents {
            self.current()?;
            let response = client
                .accept(intent)
                .await
                .map_err(|_| EnrollmentError::Unconfirmed)?;
            if !matches!(
                response.outcome,
                PublicationOutcome::Applied | PublicationOutcome::Duplicate
            ) {
                return Err(EnrollmentError::Rejected);
            }
            let delivery = response.delivery.ok_or(EnrollmentError::Unconfirmed)?;
            accepted.push(AcceptedMembership {
                record_id: intent.envelope.record_id,
                authority_sequence: delivery.sequence.get(),
            });
        }
        let final_state = self.snapshot(client).await?;
        self.check_baseline(&final_state, &intents)?;
        for acknowledgement in &accepted {
            if final_state.records.iter().any(|record| {
                record.envelope.publisher == self.plan.authority
                    && record.envelope.record_id == acknowledgement.record_id
                    && record.sequence.get() != acknowledgement.authority_sequence
            }) {
                return Err(EnrollmentError::InvalidProof);
            }
        }
        let confirmed_published_records = intents
            .iter()
            .filter(|intent| {
                final_state
                    .records
                    .iter()
                    .any(|delivery| matches_intent(delivery, intent))
            })
            .map(|intent| intent.envelope.record_id)
            .collect::<Vec<_>>();
        Ok(EnrollmentReport {
            review_sha256: digest.as_hex().into(),
            status: if confirmed_published_records.len() == intents.len() {
                "published_proven"
            } else {
                "accepted_publication_pending"
            },
            accepted,
            confirmed_published_records,
            observed: self.inspection(&final_state)?,
        })
    }

    fn check_baseline(&self, snapshot: &Snapshot, intents: &[PublicationIntent]) -> Result<()> {
        let baseline = self.plan.expected_sequence as usize;
        if snapshot.records.len() < baseline {
            return Err(EnrollmentError::ChangedHistory);
        }
        let tip = if baseline == 0 {
            None
        } else {
            Some(
                audit_digest(&snapshot.receipts[baseline - 1])
                    .map_err(|_| EnrollmentError::InvalidProof)?,
            )
        };
        if tip != self.plan.expected_tip
            || snapshot.records[baseline..]
                .iter()
                .any(|r| !intents.iter().any(|i| matches_intent(r, i)))
        {
            return Err(EnrollmentError::ChangedHistory);
        }
        Ok(())
    }

    async fn snapshot(&self, client: &GatewayClient) -> Result<Snapshot> {
        let mut records = Vec::new();
        let mut receipts = Vec::new();
        let mut after = 0;
        let mut target = None;
        let mut bytes = 0;
        loop {
            let limit = target.map_or(32, |head: u32| (head - after).min(32));
            if limit == 0 {
                break;
            }
            let page = client
                .pull_diagnostic(after, limit)
                .await
                .map_err(map_pull_failure)?;
            let head = *target.get_or_insert(page.published_through);
            if head as usize > MAX_HISTORY_RECORDS || page.published_through < head {
                return Err(EnrollmentError::InvalidProof);
            }
            bytes += CanonicalJsonCodec::encode(&page)
                .map_err(|_| EnrollmentError::InvalidProof)?
                .len();
            if bytes > MAX_HISTORY_BYTES {
                return Err(EnrollmentError::InvalidProof);
            }
            for record in page.records {
                after = record.sequence.get();
                records.push(record);
            }
            receipts.extend(page.receipts);
            if after == head {
                break;
            }
        }
        ReceiptTrust::new(self.receipt_keys.clone())
            .map_err(|_| EnrollmentError::Configuration)?
            .verify_history(self.plan.authority, &records, &receipts)
            .map_err(|_| EnrollmentError::InvalidHistoryProof)?;
        let mut state = PublicationState::new(self.plan.space, self.plan.authority);
        for record in &records {
            if state.apply(record.envelope.publisher, record.clone()) != PublicationOutcome::Applied
            {
                return Err(EnrollmentError::InvalidHistoryReplay);
            }
        }
        Ok(Snapshot {
            records,
            receipts,
            state,
        })
    }

    fn inspection(&self, snapshot: &Snapshot) -> Result<EnrollmentInspection> {
        Ok(EnrollmentInspection {
            published_through: snapshot.state.sequence(),
            published_tip: snapshot
                .receipts
                .last()
                .map(audit_digest)
                .transpose()
                .map_err(|_| EnrollmentError::InvalidProof)?,
            members: self
                .plan
                .members
                .iter()
                .map(|member| MemberInspection {
                    member: member.member,
                    capabilities: [
                        SpaceCapability::Pull,
                        SpaceCapability::Publish,
                        SpaceCapability::AskExpertise,
                        SpaceCapability::AnswerExpertise,
                    ]
                    .into_iter()
                    .filter(|c| snapshot.state.has_capability(member.member, *c))
                    .collect(),
                })
                .collect(),
        })
    }
}

fn map_pull_failure(error: PullFailure) -> EnrollmentError {
    match error {
        PullFailure::Configuration => EnrollmentError::Configuration,
        PullFailure::Unavailable => EnrollmentError::GatewayUnavailable,
        PullFailure::Unauthorized => EnrollmentError::GatewayUnauthorized,
        PullFailure::Status => EnrollmentError::InvalidGatewayStatus,
        PullFailure::Encoding => EnrollmentError::InvalidPageEncoding,
        PullFailure::PageProof(PageVerificationError::Binding) | PullFailure::Binding => {
            EnrollmentError::InvalidPageBinding
        }
        PullFailure::PageProof(PageVerificationError::Policy) => EnrollmentError::InvalidPagePolicy,
        PullFailure::PageProof(PageVerificationError::Signature) => {
            EnrollmentError::InvalidPageSignature
        }
        PullFailure::PageProof(PageVerificationError::Receipt) => {
            EnrollmentError::InvalidPageReceipt
        }
        PullFailure::PageProof(PageVerificationError::Chain) => EnrollmentError::InvalidPageChain,
        PullFailure::Ordering => EnrollmentError::InvalidPageOrder,
    }
}

fn matches_intent(delivery: &PublicationDelivery, intent: &PublicationIntent) -> bool {
    delivery.envelope == intent.envelope && delivery.locator == intent.locator
}

#[cfg(test)]
mod tests;
