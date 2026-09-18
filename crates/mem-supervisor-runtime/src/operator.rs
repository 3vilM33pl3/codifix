//! Operator-owned, single-work execution outside the hardened application.
//! There is no listener, discovery, provisioning, scheduler or model retry.
//! Only an already provisioned private Workshop manifest can authorize work.
use crate::{
    launcher::{self, LaunchError, RunControl},
    pi_broker::GuardedBrokerJob,
    transport::OpenAiTransport,
    work::{PinnedWork, WorkError, WorkStep},
};
use mem_supervisor::action::ActionState;
use serde::Serialize;
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Completed,
    Stored,
    ExistingWork,
    CredentialUnavailable,
    AuthorityUnavailable,
    StorageUnavailable,
    InterventionRequired,
    Stopped,
    ArtifactRejected,
    SandboxUnavailable,
    CleanupUnconfirmed,
    DeliveryUnconfirmed,
}

/// Only metadata can reach stdout or the service journal. No provider errors,
/// database URLs, source text, questions or private proposal bodies.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub action_id: Uuid,
    pub attempt_id: Uuid,
    pub outcome: Outcome,
    pub action_state: Option<ActionState>,
}

fn report(work: &PinnedWork, outcome: Outcome, action_state: Option<ActionState>) -> Report {
    Report {
        action_id: work.action_id(),
        attempt_id: work.attempt_id(),
        outcome,
        action_state,
    }
}

pub async fn observe(work: &PinnedWork, pool: PgPool) -> Report {
    match tokio::time::timeout(Duration::from_secs(15), work.observe(pool)).await {
        Ok(Ok(state)) => report(
            work,
            match state {
                ActionState::Verified => Outcome::Stored,
                ActionState::Observed
                | ActionState::Proposed
                | ActionState::Authorized
                | ActionState::Queued
                | ActionState::Running => Outcome::ExistingWork,
                _ => Outcome::InterventionRequired,
            },
            Some(state),
        ),
        _ => report(work, Outcome::StorageUnavailable, None),
    }
}

/// One explicit invocation, at most one physical launch. Existing running or
/// terminal work is inspected, never reissued. Missing credentials still allow
/// lease recovery without reading the approved source contents.
pub async fn run(
    work: &PinnedWork,
    pool: PgPool,
    transport: Option<OpenAiTransport>,
    control: RunControl,
) -> Report {
    let observation = observe(work, pool.clone()).await;
    if control.stopped() {
        return report(work, Outcome::Stopped, observation.action_state);
    }
    if observation.outcome != Outcome::ExistingWork
        || observation.action_state == Some(ActionState::Running)
    {
        return observation;
    }
    let Some(transport) = transport else {
        return report(
            work,
            Outcome::CredentialUnavailable,
            observation.action_state,
        );
    };
    // Observed -> Proposed -> Authorized -> Queued -> claimed. There cannot be
    // an unbounded scheduling loop or an automatic second model invocation.
    for _ in 0..4 {
        let step = tokio::select! {
            _=control.cancelled()=>return report(work,Outcome::Stopped,None),
            result=tokio::time::timeout(Duration::from_secs(15),work.step(pool.clone(),control.clone()))=>result,
        };
        match step {
            Ok(Ok(WorkStep::State(state))) => {
                if !matches!(
                    state,
                    ActionState::Proposed | ActionState::Authorized | ActionState::Queued
                ) {
                    return observe(work, pool).await;
                }
            }
            Ok(Ok(WorkStep::Claimed(claimed))) => {
                let job = Arc::new(GuardedBrokerJob {
                    store: claimed.store.clone(),
                    lease: claimed.lease.clone(),
                    execution: claimed.execution.clone(),
                    request: Arc::clone(&claimed.request),
                    transport,
                    disclosure: claimed.disclosure.clone(),
                });
                let launched = launcher::launch(
                    claimed.sandbox.clone(),
                    Arc::clone(&claimed.request),
                    job,
                    control.clone(),
                )
                .await;
                let confirmed = match launched {
                    Ok(hash) => claimed.confirm(hash).await,
                    Err(error) => Err(error),
                };
                let outcome = match confirmed {
                    Ok(()) => Outcome::Completed,
                    Err(LaunchError::Files | LaunchError::Artifact) => Outcome::ArtifactRejected,
                    Err(LaunchError::Unavailable) => Outcome::SandboxUnavailable,
                    Err(LaunchError::Cleanup) => Outcome::CleanupUnconfirmed,
                    Err(LaunchError::Stopped) => Outcome::Stopped,
                    Err(LaunchError::Deadline | LaunchError::Child) => Outcome::DeliveryUnconfirmed,
                };
                if outcome != Outcome::Completed {
                    control.stop();
                    // Keep the runtime alive for a previously dispatched broker
                    // to settle actual billing. No disclosure is now permitted.
                    // If the process dies, the existing lease stays recoverable.
                    let until = tokio::time::Instant::now() + Duration::from_secs(185);
                    loop {
                        let current = observe(work, pool.clone()).await;
                        if current
                            .action_state
                            .is_some_and(|s| s != ActionState::Running)
                            || tokio::time::Instant::now() >= until
                        {
                            return report(work, outcome, current.action_state);
                        }
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
                return report(work, outcome, Some(ActionState::Verified));
            }
            Ok(Err(WorkError::Stopped)) => return report(work, Outcome::Stopped, None),
            Ok(Err(WorkError::Authority)) => {
                return report(work, Outcome::AuthorityUnavailable, None);
            }
            _ => return report(work, Outcome::StorageUnavailable, None),
        }
    }
    report(work, Outcome::InterventionRequired, None)
}
