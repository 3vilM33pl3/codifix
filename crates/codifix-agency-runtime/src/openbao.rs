use crate::{
    config::Config,
    store::{Store, StoredTask, fingerprint},
};
use anyhow::{Result, ensure};
use codifix_agency::{ResourceScope, Usage};
use serde_json::{Value, json};
use uuid::Uuid;
pub async fn recover(store: &Store, cfg: &Config, task: &StoredTask) -> Result<Value> {
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
    let mut observations = Vec::new();
    let result =
        codifix_agency_openbao::transport::controller_tick(&controller, |o| observations.push(o))
            .await;
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
    let status = result?;
    ensure!(!status.sealed, "OpenBao remains sealed");
    Ok(json!({"controller":profile,"sealed":status.sealed,"observations":observations}))
}
