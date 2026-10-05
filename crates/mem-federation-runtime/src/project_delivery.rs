// SPDX-License-Identifier: AGPL-3.0-or-later
//! Origin credential transport. This does not authorize a recipient device.
use crate::{InstanceSync, Result, SignedProjectPage};
use mem_federation::publication::PublicationState;
use mem_federation::subscription::{ProjectFeedRegistration, ProjectPageRequest};
use mem_federation_store::{AuditReceipt, ProjectBindingReceipt, ProjectPage};

impl InstanceSync {
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
