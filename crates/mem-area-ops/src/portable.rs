// SPDX-License-Identifier: AGPL-3.0-or-later
//! Single-area transport using an independently trusted signed-handoff digest.
use crate::*;

#[allow(clippy::too_many_arguments)]
pub fn export(
    inputs: &Inputs,
    prepared: &Path,
    prepared_sha256: &str,
    budget: RetainedTargetBudget,
    area: AreaId,
    target: Uuid,
    output: &Path,
) -> Result<serde_json::Value> {
    let mut receipt = Input::open(prepared, RECEIPT_LIMIT)?;
    let (expected, digest): (Prepared, _) = json(&mut receipt, RECEIPT_LIMIT)?;
    if digest != prepared_sha256 {
        return Err(Failure::Binding);
    }
    let (migration, actual) = materialize(inputs, &mut std::io::sink(), budget)?;
    if actual != expected {
        return Err(Failure::Binding);
    }
    let bytes = mem_area::portable::export(migration.partition(area), target)?;
    receipt.unchanged()?;
    let directory = Directory::open(output)?;
    directory.ensure_absent(&["portable-partition.partial", "portable-partition.json"])?;
    let mut file = directory.create("portable-partition.partial")?;
    file.write_all(&bytes).map_err(|_| Failure::Output)?;
    directory.publish(
        "portable-partition.partial",
        "portable-partition.json",
        &file,
    )?;
    Ok(
        serde_json::json!({"status":"exported_only_not_applied","area":area,"target_instance":target,"bundle_sha256":hash(&bytes),"partition_sha256":migration.partition(area).partition_sha256(),"bytes":bytes.len(),"source_review_trust":"operator must convey bundle digest through signed handoff independently"}),
    )
}

pub struct Request<'a> {
    pub bundle: &'a Path,
    pub bundle_sha256: &'a str,
    pub target: &'a Path,
    pub target_sha256: &'a str,
    pub operation_id: Uuid,
    pub execute: bool,
    pub writers_stopped: bool,
    pub archive_backed_up: bool,
    pub preview: Option<(&'a Path, &'a str)>,
}

pub async fn reconcile(request: Request<'_>) -> Result<serde_json::Value> {
    if request.operation_id.is_nil()
        || !valid_digest(request.bundle_sha256)
        || !valid_digest(request.target_sha256)
        || (request.execute
            && (!request.writers_stopped
                || !request.archive_backed_up
                || request.preview.is_none()))
    {
        return Err(Failure::Target);
    }
    let mut input = Input::open(
        request.bundle,
        mem_area::portable::MAX_PORTABLE_BYTES as u64,
    )?;
    let mut bytes = Vec::new();
    (&mut input.file)
        .take(mem_area::portable::MAX_PORTABLE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::PrivateFiles)?;
    let mut descriptor = Input::open(request.target, TARGET_LIMIT)?;
    let (target, digest): (Target, _) = json(&mut descriptor, TARGET_LIMIT)?;
    if digest != request.target_sha256 {
        return Err(Failure::Binding);
    }
    let partition = mem_area::portable::import(
        &bytes,
        request.bundle_sha256,
        target.instance_id,
        target.area,
    )?;
    let expected = if let Some((path, digest)) = request.preview {
        let mut input = Input::open(path, MANIFEST_LIMIT)?;
        let (value, _): (mem_area_store::reconciliation::Preview, _) =
            json(&mut input, MANIFEST_LIMIT)?;
        if value.digest()? != digest
            || value.target_instance != target.instance_id
            || value.area != target.area
        {
            return Err(Failure::Binding);
        }
        input.unchanged()?;
        Some(value)
    } else {
        None
    };
    input.unchanged()?;
    descriptor.unchanged()?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(target.options()?)
        .await
        .map_err(|_| Failure::Connection)?;
    let result = if request.execute {
        mem_area_store::reconciliation::reconcile(
            &pool,
            request.operation_id,
            &partition,
            &expected.ok_or(Failure::Binding)?,
            true,
        )
        .await
        .and_then(|v| serde_json::to_value(v).map_err(|_| mem_area_store::ImportError::Invalid))
    } else {
        mem_area_store::reconciliation::preview(&pool, target.instance_id, &partition)
            .await
            .and_then(|v| serde_json::to_value(v).map_err(|_| mem_area_store::ImportError::Invalid))
    };
    pool.close().await;
    Ok(result?)
}
