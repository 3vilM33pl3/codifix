// SPDX-License-Identifier: AGPL-3.0-or-later
//! Curated-only laptop queue, transaction hooks and fenced delivery receipts.
use crate::{Error, Result};
use mem_record::*;
use sqlx::{Postgres, Row, Transaction, types::Json};
use uuid::Uuid;

pub async fn bind_project(
    tx: &mut Transaction<'_, Postgres>,
    local_project: Uuid,
    binding: &LaptopContributionBinding,
) -> Result<()> {
    binding
        .validate()
        .map_err(|_| Error::Invalid("curated device binding"))?;
    let saved: Option<Json<LaptopContributionBinding>> = sqlx::query_scalar(
        "SELECT binding FROM laptop_curated_bindings WHERE local_project_id=$1 FOR UPDATE",
    )
    .bind(local_project)
    .fetch_optional(&mut **tx)
    .await?;
    if saved.as_ref().is_some_and(|v| v.0 != *binding) {
        return Err(Error::Conflict);
    }
    let result=sqlx::query("INSERT INTO laptop_curated_bindings(local_project_id,binding,enabled) VALUES($1,$2,TRUE) ON CONFLICT(local_project_id) DO UPDATE SET enabled=TRUE WHERE laptop_curated_bindings.binding=EXCLUDED.binding").bind(local_project).bind(Json(binding)).execute(&mut **tx).await?;
    if result.rows_affected() != 1 {
        return Err(Error::Conflict);
    }
    Ok(())
}
pub struct ClaimedContribution {
    pub contribution: CuratedDeviceContribution,
    pub lease_token: Uuid,
    pub request_sha256: String,
}
async fn block_ineligible(tx: &mut Transaction<'_, Postgres>, operation: Uuid) -> Result<()> {
    sqlx::query("UPDATE laptop_curated_outbox SET state='blocked',blocked_reason='invalid_curated_content',lease_token=NULL,lease_until=NULL,updated_at=clock_timestamp() WHERE operation_id=$1")
        .bind(operation).execute(&mut **tx).await?;
    Ok(())
}
/// One queue row is fenced at a time; no network call belongs in this transaction.
pub async fn claim(
    tx: &mut Transaction<'_, Postgres>,
    binding: &LaptopContributionBinding,
) -> Result<Option<ClaimedContribution>> {
    binding
        .validate()
        .map_err(|_| Error::Invalid("claim binding"))?;
    let row=sqlx::query("SELECT q.*,m.canonical_id,m.version_no,m.canonical_text,m.summary,m.memory_type,m.importance,m.confidence,m.created_at,EXISTS(SELECT 1 FROM memory_canonical_state cs JOIN memory_entries latest ON latest.canonical_id=cs.canonical_id WHERE cs.canonical_id=m.canonical_id AND cs.status='active' AND latest.version_no=(SELECT max(version_no) FROM memory_entries WHERE canonical_id=m.canonical_id) AND NOT latest.is_tombstone) AS eligible FROM laptop_curated_outbox q JOIN memory_entries m ON m.id=q.local_memory_id JOIN laptop_curated_bindings b ON b.local_project_id=q.local_project_id WHERE b.enabled AND q.binding=$1 AND b.binding=$1 AND (q.state='queued' OR (q.state='running' AND q.lease_until<clock_timestamp())) ORDER BY q.updated_at,q.operation_id LIMIT 1 FOR UPDATE OF q SKIP LOCKED").bind(Json(binding)).fetch_optional(&mut **tx).await?;
    let Some(row) = row else { return Ok(None) };
    let operation: Uuid = row.try_get("operation_id")?;
    if !row.try_get::<bool, _>("eligible")? {
        block_ineligible(tx, operation).await?;
        return Ok(None);
    }
    let payload: Option<Json<CuratedDeviceContribution>> = match row.try_get("payload") {
        Ok(value) => value,
        Err(_) => {
            block_ineligible(tx, operation).await?;
            return Ok(None);
        }
    };
    let contribution = if let Some(payload) = payload {
        payload.0
    } else {
        let canonical: Uuid = row.try_get("canonical_id")?;
        let version: i32 = row.try_get("version_no")?;
        let origin:Option<(Uuid,i32)>=sqlx::query_as("SELECT owner_canonical_id,owner_version FROM laptop_curated_origins WHERE local_canonical_id=$1").bind(canonical).fetch_optional(&mut **tx).await?;
        if version > 1 && origin.is_none() {
            let predecessor:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM laptop_curated_outbox q JOIN memory_entries m ON m.id=q.local_memory_id WHERE m.canonical_id=$1 AND m.version_no<$2 AND q.state<>'blocked')")
                .bind(canonical).bind(version).fetch_one(&mut **tx).await?;
            if predecessor {
                // Previous local versions must reach their origin before corrections.
                // Leave queued; ordering by updated_at lets other work make progress.
                sqlx::query("UPDATE laptop_curated_outbox SET state='queued',lease_token=NULL,lease_until=NULL,updated_at=clock_timestamp() WHERE operation_id=$1").bind(operation).execute(&mut **tx).await?;
                return Ok(None);
            }
            // Earlier invalid/withdrawn blocked versions remain local history.
            // The first eligible content starts independent owner version one.
        }
        let memory_type =
            match serde_json::from_value(serde_json::Value::String(row.try_get("memory_type")?)) {
                Ok(value) => value,
                Err(_) => {
                    block_ineligible(tx, operation).await?;
                    return Ok(None);
                }
            };
        let content = CuratedDeviceContent {
            canonical_text: row.try_get("canonical_text")?,
            summary: row.try_get("summary")?,
            memory_type,
            importance: row.try_get("importance")?,
            confidence: row.try_get("confidence")?,
        };
        CuratedDeviceContribution {
            schema_version: 2,
            operation_id: operation,
            binding: row
                .try_get::<Json<LaptopContributionBinding>, _>("binding")?
                .0,
            local_memory_id: row.try_get("local_memory_id")?,
            authored_at: row.try_get("created_at")?,
            change: match origin {
                Some((target_canonical_id, expected_version)) => CuratedDeviceChange::Correct {
                    target_canonical_id,
                    expected_version,
                    content,
                },
                None => CuratedDeviceChange::Add { content },
            },
        }
    };
    if contribution.validate().is_err() {
        // Local curation supports larger content than the device wire contract.
        // Commit a fixed actionable status rather than rolling back this claim
        // repeatedly and preventing every later queued item from progressing.
        block_ineligible(tx, operation).await?;
        return Ok(None);
    }
    let lease = Uuid::new_v4();
    sqlx::query("UPDATE laptop_curated_outbox SET state='running',payload=$2,lease_token=$3,lease_until=clock_timestamp()+interval '120 seconds',updated_at=clock_timestamp() WHERE operation_id=$1").bind(operation).bind(Json(&contribution)).bind(lease).execute(&mut **tx).await?;
    use sha2::{Digest, Sha256};
    let request_sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json_canonicalizer::to_vec(&contribution)?)
    );
    Ok(Some(ClaimedContribution {
        contribution,
        lease_token: lease,
        request_sha256,
    }))
}
pub async fn acknowledge(
    tx: &mut Transaction<'_, Postgres>,
    claim: &ClaimedContribution,
    receipt: &DeviceContributionReceipt,
) -> Result<()> {
    if receipt.operation_id != claim.contribution.operation_id
        || receipt.binding != claim.contribution.binding
        || receipt.local_memory_id != claim.contribution.local_memory_id
        || receipt.schema_version != 2
        || receipt.request_sha256 != claim.request_sha256
    {
        return Err(Error::Conflict);
    }
    let status = match receipt.outcome {
        DeviceContributionOutcome::Accepted => "accepted",
        DeviceContributionOutcome::Proposed => "proposed",
        DeviceContributionOutcome::Conflict => "conflict",
    };
    let canonical:Option<Uuid>=sqlx::query_scalar("UPDATE laptop_curated_outbox q SET state=$4,receipt=$3,lease_token=NULL,lease_until=NULL,updated_at=clock_timestamp() WHERE q.operation_id=$1 AND q.state='running' AND q.lease_token=$2 AND q.lease_until>clock_timestamp() RETURNING (SELECT canonical_id FROM memory_entries WHERE id=q.local_memory_id)")
        .bind(receipt.operation_id).bind(claim.lease_token).bind(Json(receipt)).bind(status).fetch_optional(&mut **tx).await?;
    let canonical = canonical.ok_or(Error::StaleLease)?;
    if receipt.outcome == DeviceContributionOutcome::Accepted {
        let owner = receipt
            .owner_canonical_id
            .ok_or(Error::Invalid("accepted canonical missing"))?;
        let version = receipt
            .owner_version
            .ok_or(Error::Invalid("accepted version missing"))?;
        let result=sqlx::query("INSERT INTO laptop_curated_origins(local_canonical_id,owner_canonical_id,owner_version) VALUES($1,$2,$3) ON CONFLICT(local_canonical_id) DO UPDATE SET owner_version=GREATEST(laptop_curated_origins.owner_version,EXCLUDED.owner_version) WHERE laptop_curated_origins.owner_canonical_id=EXCLUDED.owner_canonical_id").bind(canonical).bind(owner).bind(version).execute(&mut **tx).await?;
        if result.rows_affected() != 1 {
            return Err(Error::Conflict);
        }
    }
    Ok(())
}
/// Transport failures preserve the exact frozen payload for the next retry.
pub async fn release(
    tx: &mut Transaction<'_, Postgres>,
    claim: &ClaimedContribution,
    blocked: bool,
) -> Result<()> {
    let result=sqlx::query("UPDATE laptop_curated_outbox SET state=$3,blocked_reason=CASE WHEN $3='blocked' THEN 'authority_denied' ELSE NULL END,lease_token=NULL,lease_until=NULL,updated_at=clock_timestamp() WHERE operation_id=$1 AND lease_token=$2 AND state='running' AND lease_until>clock_timestamp()")
        .bind(claim.contribution.operation_id).bind(claim.lease_token).bind(if blocked {"blocked"} else {"queued"}).execute(&mut **tx).await?;
    if result.rows_affected() != 1 {
        return Err(Error::StaleLease);
    }
    Ok(())
}

/// Trusted owner proposal decision hook; caller owns authorization, mutation,
/// event append and commit. Other proposal families have no device receipt.
pub async fn record_decision(
    tx: &mut Transaction<'_, Postgres>,
    proposal: Uuid,
    decision: DeviceContributionDecision,
    memory: Option<Uuid>,
    reviewer: &str,
) -> Result<()> {
    let receipt:Option<Json<DeviceContributionReceipt>>=sqlx::query_scalar("SELECT receipt_json FROM device_curated_contribution_receipts WHERE receipt_json->>'proposal_id'=$1 FOR UPDATE").bind(proposal.to_string()).fetch_optional(&mut **tx).await?;
    let Some(Json(receipt)) = receipt else {
        return Ok(());
    };
    if reviewer.is_empty() || reviewer.len() > 512 || reviewer.chars().any(char::is_control) {
        return Err(Error::Invalid("proposal reviewer identity"));
    }
    let (owner_memory_id, owner_canonical_id, owner_version) = if decision
        == DeviceContributionDecision::Approved
    {
        let memory = memory.ok_or(Error::Invalid("approved device correction memory missing"))?;
        let row: (Uuid, i32) = sqlx::query_as(
            "SELECT canonical_id,version_no FROM memory_entries WHERE id=$1 AND project_id=$2",
        )
        .bind(memory)
        .bind(receipt.binding.scope.project_id)
        .fetch_one(&mut **tx)
        .await?;
        if Some(row.0) != receipt.owner_canonical_id {
            return Err(Error::Conflict);
        }
        (Some(memory), Some(row.0), Some(row.1))
    } else {
        (None, None, None)
    };
    let resolution = DeviceContributionResolution {
        operation_id: receipt.operation_id,
        proposal_id: proposal,
        request_sha256: receipt.request_sha256,
        decision,
        owner_memory_id,
        owner_canonical_id,
        owner_version,
        reviewer: reviewer.into(),
        decided_at: sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut **tx)
            .await?,
    };
    sqlx::query("INSERT INTO device_curated_contribution_resolutions(operation_id,proposal_id,resolution_json) VALUES($1,$2,$3)").bind(resolution.operation_id).bind(proposal).bind(Json(resolution)).execute(&mut **tx).await?;
    Ok(())
}

/// A trusted authenticated transport supplies this owner outcome. An immutable
/// original receipt must match locally before any origin/version advances.
pub async fn reconcile_resolution(
    tx: &mut Transaction<'_, Postgres>,
    binding: &LaptopContributionBinding,
    status: &DeviceContributionStatus,
) -> Result<()> {
    let Some(resolution) = &status.resolution else {
        return Ok(());
    };
    let receipt = &status.receipt;
    if &receipt.binding != binding
        || resolution.operation_id != receipt.operation_id
        || Some(resolution.proposal_id) != receipt.proposal_id
        || resolution.request_sha256 != receipt.request_sha256
    {
        return Err(Error::Conflict);
    }
    let row=sqlx::query("SELECT q.*,m.canonical_id FROM laptop_curated_outbox q JOIN memory_entries m ON m.id=q.local_memory_id WHERE q.operation_id=$1 AND q.binding=$2 FOR UPDATE OF q").bind(receipt.operation_id).bind(Json(binding)).fetch_optional(&mut **tx).await?.ok_or(Error::Conflict)?;
    let saved: Json<DeviceContributionReceipt> = row.try_get("receipt")?;
    if saved.0 != *receipt {
        return Err(Error::Conflict);
    }
    let previous: Option<Json<DeviceContributionResolution>> = row.try_get("resolution_json")?;
    if let Some(previous) = previous {
        return if previous.0 == *resolution {
            Ok(())
        } else {
            Err(Error::Conflict)
        };
    }
    let state: String = row.try_get("state")?;
    if !matches!(state.as_str(), "proposed" | "conflict") {
        return Err(Error::Conflict);
    }
    let canonical: Uuid = row.try_get("canonical_id")?;
    let (state, reason) = match resolution.decision {
        DeviceContributionDecision::Approved => {
            let owner = resolution
                .owner_canonical_id
                .filter(|v| !v.is_nil())
                .ok_or(Error::Invalid("approved canonical missing"))?;
            let version = resolution
                .owner_version
                .filter(|v| *v > 0)
                .ok_or(Error::Invalid("approved version missing"))?;
            if Some(owner) != receipt.owner_canonical_id
                || resolution.owner_memory_id.is_none_or(|v| v.is_nil())
            {
                return Err(Error::Conflict);
            }
            let result=sqlx::query("INSERT INTO laptop_curated_origins(local_canonical_id,owner_canonical_id,owner_version) VALUES($1,$2,$3) ON CONFLICT(local_canonical_id) DO UPDATE SET owner_version=GREATEST(laptop_curated_origins.owner_version,EXCLUDED.owner_version) WHERE laptop_curated_origins.owner_canonical_id=EXCLUDED.owner_canonical_id").bind(canonical).bind(owner).bind(version).execute(&mut **tx).await?;
            if result.rows_affected() != 1 {
                return Err(Error::Conflict);
            }
            ("accepted", None)
        }
        DeviceContributionDecision::Rejected => {
            if resolution.owner_memory_id.is_some()
                || resolution.owner_canonical_id.is_some()
                || resolution.owner_version.is_some()
            {
                return Err(Error::Conflict);
            }
            ("blocked", Some("proposal_rejected"))
        }
    };
    sqlx::query("UPDATE laptop_curated_outbox SET state=$2,blocked_reason=$3,resolution_json=$4,updated_at=clock_timestamp() WHERE operation_id=$1").bind(receipt.operation_id).bind(state).bind(reason).bind(Json(resolution)).execute(&mut **tx).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn curation_queue_commits_atomically_and_stale_claims_cannot_ack() {
        let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let project = Uuid::new_v4();
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'device-local','Device local','/synthetic')").bind(project).execute(&db.pool).await.unwrap();
        let binding = LaptopContributionBinding {
            scope: LaptopContributionScope {
                company_id: Uuid::new_v4(),
                area: OnboardingArea::Workshop,
                owner_instance_id: Uuid::new_v4(),
                project_id: Uuid::new_v4(),
                space_id: Uuid::new_v4(),
            },
            device_id: Uuid::new_v4(),
            manifest_revision: 1,
            manifest_sha256: "a".repeat(64),
        };
        let mut tx = db.pool.begin().await.unwrap();
        bind_project(&mut tx, project, &binding).await.unwrap();
        tx.commit().await.unwrap();
        let version = Uuid::new_v4();
        let sql = "INSERT INTO memory_entries(id,project_id,canonical_id,version_no,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,'curated fact','summary','implementation','project',3,0.8,now(),now(),to_tsvector('english','curated fact'))";
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query(sql)
            .bind(version)
            .bind(project)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(
            0,
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM laptop_curated_outbox")
                .fetch_one(&db.pool)
                .await
                .unwrap()
        );
        sqlx::query(sql)
            .bind(version)
            .bind(project)
            .execute(&db.pool)
            .await
            .unwrap();
        let mut other = binding.clone();
        other.scope.project_id = Uuid::new_v4();
        let mut tx = db.pool.begin().await.unwrap();
        assert!(claim(&mut tx, &other).await.unwrap().is_none());
        tx.commit().await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        let first = claim(&mut tx, &binding).await.unwrap().unwrap();
        tx.commit().await.unwrap();
        assert_eq!(first.contribution.local_memory_id, version);
        let mut tx = db.pool.begin().await.unwrap();
        assert!(claim(&mut tx, &binding).await.unwrap().is_none());
        tx.commit().await.unwrap();
        sqlx::query(
            "UPDATE laptop_curated_outbox SET lease_until=clock_timestamp()-interval '1 second'",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        let second = claim(&mut tx, &binding).await.unwrap().unwrap();
        tx.commit().await.unwrap();
        assert_eq!(first.contribution, second.contribution);
        assert_ne!(first.lease_token, second.lease_token);
        let receipt = DeviceContributionReceipt {
            schema_version: 2,
            operation_id: version,
            binding,
            local_memory_id: version,
            request_sha256: second.request_sha256.clone(),
            human_principal_id: Uuid::new_v4(),
            outcome: DeviceContributionOutcome::Accepted,
            owner_memory_id: Some(Uuid::new_v4()),
            owner_canonical_id: Some(Uuid::new_v4()),
            owner_version: Some(1),
            proposal_id: None,
            committed_at: chrono::Utc::now(),
        };
        let mut tx = db.pool.begin().await.unwrap();
        let mut altered = receipt.clone();
        altered.request_sha256 = "f".repeat(64);
        assert!(matches!(
            acknowledge(&mut tx, &second, &altered).await,
            Err(Error::Conflict)
        ));
        tx.rollback().await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        assert!(matches!(
            acknowledge(&mut tx, &first, &receipt).await,
            Err(Error::StaleLease)
        ));
        tx.rollback().await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        acknowledge(&mut tx, &second, &receipt).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            "accepted",
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM laptop_curated_outbox WHERE operation_id=$1"
            )
            .bind(version)
            .fetch_one(&db.pool)
            .await
            .unwrap()
        );
        let oversized = Uuid::new_v4();
        let valid = Uuid::new_v4();
        for (id, text) in [
            (oversized, "x".repeat(32769)),
            (valid, "eligible later curated fact".into()),
        ] {
            sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) SELECT $1,project_id,$1,1,$2,summary,memory_type,scope,importance,confidence,now(),now(),to_tsvector('english',$2) FROM memory_entries WHERE id=$3")
                .bind(id).bind(text).bind(version).execute(&db.pool).await.unwrap();
        }
        sqlx::query("UPDATE laptop_curated_outbox SET updated_at='2000-01-01T00:00:00Z' WHERE operation_id=$1").bind(oversized).execute(&db.pool).await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        assert!(claim(&mut tx, &receipt.binding).await.unwrap().is_none());
        tx.commit().await.unwrap();
        let status: (String, Option<String>) = sqlx::query_as(
            "SELECT state,blocked_reason FROM laptop_curated_outbox WHERE operation_id=$1",
        )
        .bind(oversized)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(
            status,
            ("blocked".into(), Some("invalid_curated_content".into()))
        );
        let mut tx = db.pool.begin().await.unwrap();
        let next = claim(&mut tx, &receipt.binding).await.unwrap().unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            next.contribution.local_memory_id, valid,
            "an invalid oldest item cannot poison later work"
        );
        let repaired = Uuid::new_v4();
        sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) SELECT $1,project_id,canonical_id,2,'repaired bounded fact',summary,memory_type,scope,importance,confidence,now(),now(),to_tsvector('english','repaired bounded fact') FROM memory_entries WHERE id=$2").bind(repaired).bind(oversized).execute(&db.pool).await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        let repaired_work = claim(&mut tx, &receipt.binding).await.unwrap().unwrap();
        tx.commit().await.unwrap();
        assert_eq!(repaired_work.contribution.local_memory_id, repaired);
        assert!(
            matches!(
                repaired_work.contribution.change,
                CuratedDeviceChange::Add { .. }
            ),
            "valid repair after blocked first version must start an owner version"
        );
        let original_owner = receipt.owner_canonical_id.unwrap();
        for (version_no, decision) in [
            (2, DeviceContributionDecision::Approved),
            (3, DeviceContributionDecision::Rejected),
        ] {
            let id = Uuid::new_v4();
            sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) SELECT $1,project_id,canonical_id,$2,'corrected fact',summary,memory_type,scope,importance,confidence,now(),now(),to_tsvector('english','corrected fact') FROM memory_entries WHERE id=$3").bind(id).bind(version_no).bind(version).execute(&db.pool).await.unwrap();
            let mut tx = db.pool.begin().await.unwrap();
            let work = claim(&mut tx, &receipt.binding).await.unwrap().unwrap();
            tx.commit().await.unwrap();
            assert_eq!(work.contribution.local_memory_id, id);
            let submitted = DeviceContributionReceipt {
                schema_version: 2,
                operation_id: id,
                binding: receipt.binding.clone(),
                local_memory_id: id,
                request_sha256: work.request_sha256.clone(),
                human_principal_id: receipt.human_principal_id,
                outcome: DeviceContributionOutcome::Proposed,
                owner_memory_id: None,
                owner_canonical_id: Some(original_owner),
                owner_version: Some(version_no - 1),
                proposal_id: Some(Uuid::new_v4()),
                committed_at: chrono::Utc::now(),
            };
            let mut tx = db.pool.begin().await.unwrap();
            acknowledge(&mut tx, &work, &submitted).await.unwrap();
            tx.commit().await.unwrap();
            let approved = decision == DeviceContributionDecision::Approved;
            let resolution = DeviceContributionResolution {
                operation_id: id,
                proposal_id: submitted.proposal_id.unwrap(),
                request_sha256: submitted.request_sha256.clone(),
                decision,
                owner_memory_id: approved.then(Uuid::new_v4),
                owner_canonical_id: approved.then_some(original_owner),
                owner_version: approved.then_some(version_no),
                reviewer: "synthetic-reviewer".into(),
                decided_at: chrono::Utc::now(),
            };
            let status = DeviceContributionStatus {
                receipt: submitted.clone(),
                resolution: Some(resolution),
            };
            let mut tx = db.pool.begin().await.unwrap();
            reconcile_resolution(&mut tx, &receipt.binding, &status)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            let mut tx = db.pool.begin().await.unwrap();
            reconcile_resolution(&mut tx, &receipt.binding, &status)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            let saved: Json<DeviceContributionReceipt> = sqlx::query_scalar(
                "SELECT receipt FROM laptop_curated_outbox WHERE operation_id=$1",
            )
            .bind(id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
            assert_eq!(saved.0, submitted);
            let current: i32 = sqlx::query_scalar(
                "SELECT owner_version FROM laptop_curated_origins WHERE local_canonical_id=$1",
            )
            .bind(version)
            .fetch_one(&db.pool)
            .await
            .unwrap();
            assert_eq!(current, 2, "rejected resolution must not advance origin");
            let state: String =
                sqlx::query_scalar("SELECT state FROM laptop_curated_outbox WHERE operation_id=$1")
                    .bind(id)
                    .fetch_one(&db.pool)
                    .await
                    .unwrap();
            assert_eq!(state, if approved { "accepted" } else { "blocked" });
        }
        for tombstone in [false, true] {
            let withdrawn = Uuid::new_v4();
            sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) SELECT $1,project_id,$1,1,'withdrawn fact',summary,memory_type,scope,importance,confidence,now(),now(),to_tsvector('english','withdrawn fact') FROM memory_entries WHERE id=$2").bind(withdrawn).bind(version).execute(&db.pool).await.unwrap();
            if tombstone {
                sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) SELECT $1,project_id,canonical_id,2,TRUE,'',summary,memory_type,scope,importance,confidence,now(),now(),to_tsvector('english','') FROM memory_entries WHERE id=$2").bind(Uuid::new_v4()).bind(withdrawn).execute(&db.pool).await.unwrap();
            } else {
                sqlx::query(
                    "UPDATE memory_canonical_state SET status='archived' WHERE canonical_id=$1",
                )
                .bind(withdrawn)
                .execute(&db.pool)
                .await
                .unwrap();
            }
            let mut tx = db.pool.begin().await.unwrap();
            assert!(claim(&mut tx, &receipt.binding).await.unwrap().is_none());
            tx.commit().await.unwrap();
            assert_eq!(
                "blocked",
                sqlx::query_scalar::<_, String>(
                    "SELECT state FROM laptop_curated_outbox WHERE operation_id=$1"
                )
                .bind(withdrawn)
                .fetch_one(&db.pool)
                .await
                .unwrap()
            );
        }
        db.cleanup().await.unwrap();
    }
}
