//! Explicit Workshop-local source consent. This is not a running worker or an
//! installer; the host must opt in, authenticate configuration changes and stop
//! jobs on read-only/evaluation/reload. Never construct grants from Pi input.
use crate::{
    launcher::RunControl,
    model::{PreparedRequest, Proposal},
    pi_broker::JobFuture,
    question_queue::ServiceExecutionGuard,
    transport::{DisclosureGuard, DisclosurePhase, ProposalExecution, TransportError},
};
use codifix_supervisor_store::{
    LeaseClaim, PrivateProposal, SupervisorStore,
    questions::{self, QueuedQuestion},
    sources::{self, LocalCollectionGrant},
};
use sqlx::{PgPool, Postgres, Transaction};
use std::sync::Arc;

pub struct LocalDisclosure {
    pool: PgPool,
    grant: LocalCollectionGrant,
    question: String,
    control: RunControl,
    queued: Option<(QueuedQuestion, Arc<ServiceExecutionGuard>)>,
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
            queued: None,
        }
    }

    /// Cancellation denies new disclosure and publication; in-flight provider
    /// accounting remains recoverable. A commit already started may complete.
    pub fn with_control(mut self, control: RunControl) -> Self {
        self.control = control;
        self
    }

    pub(crate) fn with_question(
        mut self,
        question: QueuedQuestion,
        guard: Arc<ServiceExecutionGuard>,
    ) -> Self {
        self.queued = Some((question, guard));
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
        if let Some((question, guard)) = &self.queued {
            guard
                .check(
                    question.submission.instance_id,
                    question.submission.profile_digest,
                )
                .map_err(|_| TransportError::Authority)?;
            questions::authorize_question_tx(tx, question)
                .await
                .map_err(|_| TransportError::Authority)?;
            let input = &question.submission;
            if input.project_id != self.grant.project_id
                || input.instance_id != self.grant.instance_id
                || input.policy_revision != self.grant.policy_revision
                || input.question != self.question
            {
                return Err(TransportError::Authority);
            }
        }
        let evidence = sources::authorize_tx(tx, &self.grant)
            .await
            .map_err(|_| TransportError::Authority)?;
        self.check_running()?;
        let prepared =
            PreparedRequest::new(&self.question, &evidence).map_err(TransportError::Model)?;
        if let Some((question, _)) = &self.queued
            && private_files_digest(prepared.digest())? != question.submission.request_digest
        {
            return Err(TransportError::Authority);
        }
        // Source/policy acquisition can wait. Credential/question expiry and
        // operator config changes during those waits must deny even counting.
        if let Some((question, guard)) = &self.queued {
            questions::authorize_question_tx(tx, question)
                .await
                .map_err(|_| TransportError::Authority)?;
            guard
                .check(
                    question.submission.instance_id,
                    question.submission.profile_digest,
                )
                .map_err(|_| TransportError::Authority)?;
        }
        self.check_running()?;
        Ok(prepared)
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
            || self
                .queued
                .as_ref()
                .is_some_and(|(question, _)| execution.attempt_id != question.attempt_id)
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

fn private_files_digest(value: &str) -> Result<[u8; 32], TransportError> {
    crate::private_files::digest(value).map_err(|_| TransportError::Authority)
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
            if self
                .queued
                .as_ref()
                .is_some_and(|(question, _)| lease.action_id() != question.submission.request_id)
            {
                return Err(TransportError::Authority);
            }
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
