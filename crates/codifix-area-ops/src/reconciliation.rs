// SPDX-License-Identifier: AGPL-3.0-or-later
//! Existing-target reconciliation is deliberately separate from fresh import.
use crate::*;
use codifix_area_store::reconciliation::{Preview, Receipt};

pub fn inspect_preview(path: &Path) -> Result<serde_json::Value> {
    let mut file = Input::open(path, MANIFEST_LIMIT)?;
    let (preview, _): (Preview, _) = json(&mut file, MANIFEST_LIMIT)?;
    Ok(
        serde_json::json!({"preview_sha256":preview.digest()?, "unresolved_conflicts":preview.conflicts()}),
    )
}

pub async fn preview(
    inputs: &Inputs,
    execution: &Execution,
    budget: RetainedTargetBudget,
) -> Result<Preview> {
    let (migration, target, pool) = validated_target(inputs, execution, budget, false).await?;
    let result = codifix_area_store::reconciliation::preview(
        &pool,
        target.instance_id,
        migration.partition(target.area),
    )
    .await;
    pool.close().await;
    Ok(result?)
}

pub async fn execute(
    inputs: &Inputs,
    execution: &Execution,
    budget: RetainedTargetBudget,
    preview_file: &Path,
    preview_sha256: &str,
) -> Result<Receipt> {
    let mut file = Input::open(preview_file, MANIFEST_LIMIT)?;
    let (preview, _): (Preview, _) = json(&mut file, MANIFEST_LIMIT)?;
    if preview.digest()? != preview_sha256 {
        return Err(Failure::Binding);
    }
    let (migration, target, pool) = validated_target(inputs, execution, budget, true).await?;
    if preview.target_instance != target.instance_id || preview.area != target.area {
        pool.close().await;
        return Err(Failure::Binding);
    }
    file.unchanged()?;
    let result = codifix_area_store::reconciliation::reconcile(
        &pool,
        execution.operation_id,
        migration.partition(target.area),
        &preview,
        execution.writers_stopped,
    )
    .await;
    pool.close().await;
    Ok(result?)
}
