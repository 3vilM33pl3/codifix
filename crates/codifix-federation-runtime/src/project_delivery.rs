// SPDX-License-Identifier: AGPL-3.0-or-later
//! Origin credential transport. This does not authorize a recipient device.
use crate::{InstanceSync, Result, SignedProjectPage};
use codifix_federation::publication::PublicationState;
use codifix_federation::subscription::{ProjectFeedRegistration, ProjectPageRequest};
use codifix_federation_store::{AuditReceipt, ProjectBindingReceipt, ProjectPage};

impl crate::GatewaySecurity {
    /// Gateway integration edges may sign only the typed registration receipt.
    pub fn sign_owner_project_binding(
        &self,
        request: ProjectFeedRegistration,
        receipt: ProjectBindingReceipt,
    ) -> Result<crate::SignedProjectBinding> {
        self.sign_project_binding(request, receipt)
    }
}

impl InstanceSync {
    /// Public administrator-pinned trust, not a gateway endpoint or credential.
    pub fn project_delivery_trust(
        &self,
    ) -> (
        codifix_federation::publication::InstanceId,
        Vec<codifix_federation_store::ReceiptKey>,
    ) {
        (self.client.authority, self.client.trust.keys().to_vec())
    }
    /// Only retained, cryptographically verified local publications; no network
    /// whole-space pull is performed on behalf of a device.
    pub async fn verified_project_sources(&self) -> Result<(PublicationState, Vec<AuditReceipt>)> {
        let (state, receipts) = self.replica.audited_snapshot().await?;
        self.client
            .trust
            .verify_history(self.client.authority, state.accepted(), &receipts)?;
        Ok((state, receipts))
    }
    pub async fn register_project_delivery(
        &self,
        request: &ProjectFeedRegistration,
    ) -> Result<ProjectBindingReceipt> {
        self.client.register_project(request).await
    }
    pub async fn project_delivery_page(
        &self,
        request: &ProjectPageRequest,
    ) -> Result<SignedProjectPage> {
        self.client.project_page(request).await
    }
    /// Repeat verification after local locks are reacquired: the short-lived
    /// gateway attestation must still be valid at the delivery commit.
    pub fn verify_project_delivery_page(
        &self,
        signed: SignedProjectPage,
        request: &ProjectPageRequest,
    ) -> Result<ProjectPage> {
        self.client
            .trust
            .verify_project_page(signed, request, self.client.authority)
    }
}
