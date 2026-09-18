//! Explicit Workshop-local source consent. This is not a running worker or an
//! installer; the host must opt in, authenticate configuration changes and stop
//! jobs on read-only/evaluation/reload. Never construct grants from Pi input.
use crate::{
    launcher::RunControl,
    model::{PreparedRequest, Proposal},
    pi_broker::JobFuture,
    transport::{DisclosureGuard, DisclosurePhase, ProposalExecution, TransportError},
};
use mem_supervisor_store::{
    LeaseClaim, PrivateProposal, SupervisorStore,
    sources::{self, LocalCollectionGrant},
};
use sqlx::{PgPool, Postgres, Transaction};

pub struct LocalDisclosure {
    pool: PgPool,
    grant: LocalCollectionGrant,
    question: String,
    control: RunControl,
}

impl LocalDisclosure {
    /// Trusted opt-in configuration only. Grant contains exact frozen sources
    /// and explicit OpenAI destination consent; no broad query or auto-upgrade.
    pub fn new(pool: PgPool, grant: LocalCollectionGrant, question: String) -> Self {
        Self {
            pool,
            grant,
            question,
            control: RunControl::default(),
        }
    }

    /// Cancellation denies new disclosure and publication; in-flight provider
    /// accounting remains recoverable. A commit already started may complete.
    pub fn with_control(mut self, control: RunControl) -> Self {
        self.control = control;
        self
    }

    fn check_running(&self) -> Result<(), TransportError> {
        if self.control.stopped() {
            Err(TransportError::Authority)
        } else {
            Ok(())
        }
    }

    pub async fn prepare(&self) -> Result<PreparedRequest, TransportError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| TransportError::Authority)?;
        let request = self.current(&mut tx).await?;
        tx.commit().await.map_err(|_| TransportError::Authority)?;
        Ok(request)
    }

    async fn current(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<PreparedRequest, TransportError> {
        self.check_running()?;
        let evidence = sources::authorize_tx(tx, &self.grant)
            .await
            .map_err(|_| TransportError::Authority)?;
        self.check_running()?;
        PreparedRequest::new(&self.question, &evidence).map_err(TransportError::Model)
    }

    async fn validate(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        execution: &ProposalExecution,
        request: &PreparedRequest,
    ) -> Result<(), TransportError> {
        if execution.instance_id != self.grant.instance_id
            || execution.project_id != self.grant.project_id
            || execution.policy_revision != self.grant.policy_revision
        {
            return Err(TransportError::Authority);
        }
        let current = self.current(tx).await?;
        if current.digest() != request.digest()
            || current.evidence_digests() != request.evidence_digests()
        {
            return Err(TransportError::Authority);
        }
        Ok(())
    }
}

impl DisclosureGuard for LocalDisclosure {
    fn authorize<'a>(
        &'a self,
        _phase: DisclosurePhase,
        execution: &'a ProposalExecution,
        request: &'a PreparedRequest,
    ) -> JobFuture<'a, Result<(), TransportError>> {
        Box::pin(async {
            let mut tx = self
                .pool
                .begin()
                .await
                .map_err(|_| TransportError::Authority)?;
            self.validate(&mut tx, execution, request).await?;
            tx.commit().await.map_err(|_| TransportError::Authority)
        })
    }
    fn publish<'a>(
        &'a self,
        lease: &'a LeaseClaim,
        execution: &'a ProposalExecution,
        request: &'a PreparedRequest,
        proposal: &'a Proposal,
    ) -> JobFuture<'a, Result<PrivateProposal, TransportError>> {
        Box::pin(async {
            let mut tx = self
                .pool
                .begin()
                .await
                .map_err(|_| TransportError::Authority)?;
            self.validate(&mut tx, execution, request).await?;
            let stored = SupervisorStore::publish_proposal_tx(
                &mut tx,
                lease,
                &execution.attempt_id,
                execution.project_id,
                proposal,
                request.evidence_digests(),
            )
            .await
            .map_err(|_| TransportError::Authority)?;
            // A journal-head wait must not carry an expiring collection grant
            // past publication. All source/auth locks are already held here.
            self.validate(&mut tx, execution, request).await?;
            // Auth/source locks and the result share this commit. Failure here
            // cannot report a durable proposal or Verified action.
            tx.commit().await.map_err(|_| TransportError::Journal)?;
            Ok(stored)
        })
    }
}
