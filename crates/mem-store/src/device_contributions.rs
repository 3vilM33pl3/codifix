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
}
/// One queue row is fenced at a time; no network call belongs in this transaction.
pub async fn claim(tx: &mut Transaction<'_, Postgres>) -> Result<Option<ClaimedContribution>> {
    let row=sqlx::query("SELECT q.*,m.canonical_id,m.version_no,m.canonical_text,m.summary,m.memory_type,m.importance,m.confidence,m.created_at FROM laptop_curated_outbox q JOIN memory_entries m ON m.id=q.local_memory_id JOIN laptop_curated_bindings b ON b.local_project_id=q.local_project_id WHERE b.enabled AND (q.state='queued' OR (q.state='running' AND q.lease_until<clock_timestamp())) ORDER BY q.updated_at,q.operation_id LIMIT 1 FOR UPDATE OF q SKIP LOCKED").fetch_optional(&mut **tx).await?;
    let Some(row) = row else { return Ok(None) };
    let operation: Uuid = row.try_get("operation_id")?;
    let payload: Option<Json<CuratedDeviceContribution>> = row.try_get("payload")?;
    let contribution = if let Some(payload) = payload {
        payload.0
    } else {
        let canonical: Uuid = row.try_get("canonical_id")?;
        let version: i32 = row.try_get("version_no")?;
        let origin:Option<(Uuid,i32)>=sqlx::query_as("SELECT owner_canonical_id,owner_version FROM laptop_curated_origins WHERE local_canonical_id=$1").bind(canonical).fetch_optional(&mut **tx).await?;
        if version > 1 && origin.is_none() {
            // Previous local versions must reach their origin before corrections.
            // Leave queued; ordering by updated_at lets other work make progress.
            sqlx::query("UPDATE laptop_curated_outbox SET state='queued',lease_token=NULL,lease_until=NULL,updated_at=clock_timestamp() WHERE operation_id=$1").bind(operation).execute(&mut **tx).await?;
            return Ok(None);
        }
        let content = CuratedDeviceContent {
            canonical_text: row.try_get("canonical_text")?,
            summary: row.try_get("summary")?,
            memory_type: serde_json::from_value(serde_json::Value::String(
                row.try_get("memory_type")?,
            ))?,
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
    contribution
        .validate()
        .map_err(|_| Error::Invalid("curated queue payload"))?;
    let lease = Uuid::new_v4();
    sqlx::query("UPDATE laptop_curated_outbox SET state='running',payload=$2,lease_token=$3,lease_until=clock_timestamp()+interval '120 seconds',updated_at=clock_timestamp() WHERE operation_id=$1").bind(operation).bind(Json(&contribution)).bind(lease).execute(&mut **tx).await?;
    Ok(Some(ClaimedContribution {
        contribution,
        lease_token: lease,
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
    let result=sqlx::query("UPDATE laptop_curated_outbox SET state=$3,lease_token=NULL,lease_until=NULL,updated_at=clock_timestamp() WHERE operation_id=$1 AND lease_token=$2 AND state='running' AND lease_until>clock_timestamp()")
        .bind(claim.contribution.operation_id).bind(claim.lease_token).bind(if blocked {"blocked"} else {"queued"}).execute(&mut **tx).await?;
    if result.rows_affected() != 1 {
        return Err(Error::StaleLease);
    }
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
        let mut tx = db.pool.begin().await.unwrap();
        let first = claim(&mut tx).await.unwrap().unwrap();
        tx.commit().await.unwrap();
        assert_eq!(first.contribution.local_memory_id, version);
        let mut tx = db.pool.begin().await.unwrap();
        assert!(claim(&mut tx).await.unwrap().is_none());
        tx.commit().await.unwrap();
        sqlx::query(
            "UPDATE laptop_curated_outbox SET lease_until=clock_timestamp()-interval '1 second'",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        let second = claim(&mut tx).await.unwrap().unwrap();
        tx.commit().await.unwrap();
        assert_eq!(first.contribution, second.contribution);
        assert_ne!(first.lease_token, second.lease_token);
        let receipt = DeviceContributionReceipt {
            schema_version: 2,
            operation_id: version,
            binding,
            local_memory_id: version,
            request_sha256: "a".repeat(64),
            human_principal_id: Uuid::new_v4(),
            outcome: DeviceContributionOutcome::Accepted,
            owner_memory_id: Some(Uuid::new_v4()),
            owner_canonical_id: Some(Uuid::new_v4()),
            owner_version: Some(1),
            proposal_id: None,
            committed_at: chrono::Utc::now(),
        };
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
        db.cleanup().await.unwrap();
    }
}
