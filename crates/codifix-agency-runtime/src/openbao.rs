use crate::{
    config::Config,
    notice::{NoticeEvent, ScenarioNotice},
    store::{Store, StoredTask, fingerprint},
};
use anyhow::{Result, ensure};
use codifix_agency::{ResourceScope, Usage};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use uuid::Uuid;

fn check_notice_recipients(
    interactive: bool,
    recipients: &[String],
    enrolled: impl Fn(&str) -> bool,
) -> Result<()> {
    let unique: BTreeSet<_> = recipients.iter().collect();
    ensure!(
        unique.len() == recipients.len(),
        "OpenBao notice recipients must be distinct"
    );
    ensure!(
        !interactive || unique.len() >= 2,
        "Interactive OpenBao recovery requires the operator and a companion notice recipient"
    );
    ensure!(
        unique.iter().all(|peer| enrolled(peer)),
        "OpenBao notice recipient is not enrolled"
    );
    Ok(())
}

pub async fn recover(store: &Store, cfg: &Config, task: &StoredTask) -> Result<Option<Value>> {
    let profile = task.request.payload["controller"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("controller profile required"))?;
    ensure!(
        task.request.resource
            == ResourceScope {
                kind: "openbao".into(),
                id: profile.into()
            },
        "OpenBao resource mismatch"
    );
    let path = cfg
        .openbao_controllers
        .get(profile)
        .ok_or_else(|| anyhow::anyhow!("controller is not enrolled"))?;
    let controller: codifix_agency_openbao::ControllerConfig =
        codifix_agency_openbao::protected::load(std::path::Path::new(path))?;
    if let Err(error) = check_notice_recipients(
        controller.peers.iter().any(|peer| peer.interactive),
        &controller.notice_peers,
        |peer| {
            cfg.social
                .as_ref()
                .is_some_and(|s| s.peers.contains_key(peer))
                && cfg
                    .peers
                    .iter()
                    .any(|p| p.id == peer && p.endpoint.is_some())
        },
    ) {
        let mut waiting = task.clone();
        waiting
            .contract
            .clarify(error.to_string(), chrono::Utc::now())?;
        store.update_contract(&waiting).await?;
        return Ok(None);
    }
    let id = Uuid::new_v5(&task.contract.id, b"openbao-recovery");
    store
        .admit_effect(
            task.contract.id,
            task.revision,
            Usage {
                calls: 1,
                turns: 1,
                ..Usage::default()
            },
            &codifix_store::effects::EffectIntent {
                id,
                project_id: Some(task.project_id),
                kind: codifix_store::effects::EffectKind::ProviderRequest,
                host_id: "agency".into(),
                resource: format!("openbao:{profile}"),
                input_fingerprint: fingerprint(&controller)?,
                actor_id: task.contract.specialist_id.clone(),
                source: "agency-openbao".into(),
            },
        )
        .await?;
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let run = async {
        let status = codifix_agency_openbao::transport::controller_tick(&controller, |o| {
            let _ = sender.send(o);
        })
        .await;
        drop(sender);
        status
    };
    let notices = async {
        use codifix_agency_openbao::Observation::*;
        let mut observations = Vec::new();
        let mut attempt = None;
        let mut last_participant = None;
        let mut failed = false;
        while let Some(observation) = receiver.recv().await {
            let (event, participant) = match &observation {
                CeremonyStarted { challenge, .. } => {
                    attempt = Some(Uuid::parse_str(challenge)?);
                    (Some(NoticeEvent::UnsealStarted), None)
                }
                ContributionRequested { custodian_id, .. } => {
                    last_participant = Some(custodian_id.clone());
                    (
                        Some(NoticeEvent::ContributionRequested),
                        last_participant.clone(),
                    )
                }
                ShareRejected { custodian_id, .. } => {
                    (Some(NoticeEvent::InvalidShare), Some(custodian_id.clone()))
                }
                AwaitingOperator { custodian_id, .. } => (
                    Some(NoticeEvent::AwaitingOperator),
                    Some(custodian_id.clone()),
                ),
                CustodianSubmitted { custodian_id, .. } => (
                    Some(NoticeEvent::ContributionAccepted),
                    Some(custodian_id.clone()),
                ),
                Escalation { code } => (
                    Some(match code.as_str() {
                        "participant_declined" => NoticeEvent::ContributionDeclined,
                        "invalid_share" => NoticeEvent::InvalidShare,
                        _ => NoticeEvent::UnsealBlocked,
                    }),
                    last_participant.clone(),
                ),
                Unsealed => (None, None), // publish recovery only after independent health readback
            };
            if let (Some(attempt), Some(event)) = (attempt, event) {
                failed |= !notify(
                    store,
                    cfg,
                    &controller,
                    task.contract.id,
                    attempt,
                    event,
                    participant,
                )
                .await;
            }
            observations.push(observation);
        }
        Ok::<_, anyhow::Error>((observations, attempt, failed))
    };
    let (result, notices) = tokio::join!(run, notices);
    let status = if result.is_ok() {
        codifix_store::effects::EffectStatus::Confirmed
    } else {
        codifix_store::effects::EffectStatus::Uncertain
    };
    codifix_store::effects::conclude(
        &store.pool,
        id,
        Uuid::new_v4(),
        status,
        codifix_store::effects::EffectReason::Observed,
    )
    .await?;
    let (observations, attempt, mut notification_failed) = notices?;
    if let Some(attempt) = attempt {
        let event = if result.is_ok() {
            NoticeEvent::UnsealedReady
        } else {
            NoticeEvent::UnsealBlocked
        };
        notification_failed |= !notify(
            store,
            cfg,
            &controller,
            task.contract.id,
            attempt,
            event,
            None,
        )
        .await;
    }
    let blocker = match result {
        Err(error) => Some(error.to_string()),
        Ok(_) if notification_failed => {
            Some("OpenBao is ready; participant notification unconfirmed.".into())
        }
        Ok(_) => None,
    };
    if let Some(blocker) = blocker {
        let mut waiting = task.clone();
        waiting.contract.clarify(blocker, chrono::Utc::now())?;
        store.update_contract(&waiting).await?;
        return Ok(None);
    }
    Ok(Some(
        json!({"controller":profile,"sealed":false,"ready":true,"observations":observations}),
    ))
}

async fn notify(
    store: &Store,
    cfg: &Config,
    controller: &codifix_agency_openbao::ControllerConfig,
    task_id: Uuid,
    attempt: Uuid,
    event: NoticeEvent,
    participant: Option<String>,
) -> bool {
    let notice = ScenarioNotice {
        task_id,
        attempt,
        subject: controller.target.expected.installation_id.clone(),
        event,
        participant,
    };
    let mut delivered = true;
    for peer in &controller.notice_peers {
        if notice.send(store, cfg, peer).await.is_err() {
            delivered = false;
        }
    }
    delivered
}

#[cfg(test)]
mod tests {
    use super::check_notice_recipients;

    #[test]
    fn interactive_recovery_rejects_missing_duplicate_or_unenrolled_recipients() {
        let enrolled = |peer: &str| matches!(peer, "operator" | "homelab");
        let check = |recipient_names: &[&str]| {
            let recipients = recipient_names
                .iter()
                .map(|id| (*id).into())
                .collect::<Vec<_>>();
            check_notice_recipients(true, &recipients, enrolled)
        };
        assert!(check(&[]).is_err());
        assert!(check(&["operator"]).is_err());
        assert!(check(&["operator", "operator"]).is_err());
        assert!(check(&["operator", "unregistered"]).is_err());
        assert!(check(&["operator", "homelab"]).is_ok());
        assert!(check_notice_recipients(false, &[], enrolled).is_ok());
    }
}
