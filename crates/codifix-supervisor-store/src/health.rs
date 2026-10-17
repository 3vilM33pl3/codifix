// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded DB-only observation. The schedule, observation, action result and
//! journal commit together. No provider, filesystem, shell or remote read.
use super::*;
use codifix_record::SupervisorHealthObservation;
use codifix_supervisor::action::KnownOutcome;
use sha2::{Digest, Sha256};

impl SupervisorStore {
    /// Trusted service opt-in is required by the caller. This common policy is
    /// only sufficient for the fixed operational counts below, never arbitrary
    /// reads, private model disclosure, remote questions or maintenance work.
    ///
    /// Because all work is inside one short transaction, a locked schedule is
    /// the execution fence: no external effect or unacknowledged worker result
    /// needs a recoverable lease. A crashed transaction rolls back the due time.
    pub async fn observe_health(
        &self,
        instance_id: Uuid,
        area: Area,
    ) -> Result<Option<SupervisorHealthObservation>> {
        non_nil(instance_id)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET LOCAL lock_timeout='2s'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL statement_timeout='5s'")
            .execute(&mut *tx)
            .await?;
        let policy = policy::lock_policy(&mut tx, area).await?;
        policy.validate(policy.revision)?;
        let binding = contexts::read_binding(&mut tx)
            .await?
            .ok_or(StoreError::Binding)?;
        if binding.instance_id != instance_id || binding.area != area {
            return Err(StoreError::Binding);
        }
        sqlx::query("INSERT INTO supervisor_health_schedule(instance_id,area) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(instance_id).bind(area_name(area)).execute(&mut *tx).await?;
        let due: DateTime<Utc> = sqlx::query_scalar("SELECT next_due_at FROM supervisor_health_schedule WHERE instance_id=$1 AND area=$2 FOR UPDATE")
            .bind(instance_id).bind(area_name(area)).fetch_one(&mut *tx).await?;
        let now = db_now(&mut tx).await?;
        if due > now {
            tx.commit().await?;
            return Ok(None);
        }
        // One SQL statement observes consistent operational counts. Successful
        // collection is not proof of backups, network, models or overall health.
        let counts = sqlx::query("SELECT (SELECT count(*) FROM projects) AS project_count, (SELECT count(*) FROM memory_canonical_state) AS canonical_count, (SELECT count(*) FROM background_jobs WHERE status='blocked') AS blocked_jobs, (SELECT count(*) FROM background_jobs WHERE status='failed') AS failed_jobs, (SELECT count(*) FROM supervisor_actions WHERE area=$1 AND state='uncertain') AS uncertain_actions")
            .bind(area_name(area)).fetch_one(&mut *tx).await?;
        let action_id = Uuid::new_v4();
        let next_due_at = db_now(&mut tx).await? + chrono::Duration::minutes(5);
        let observation = SupervisorHealthObservation {
            action_id,
            policy_revision: policy.revision,
            observed_at: now,
            next_due_at,
            project_count: counts.try_get::<i64, _>("project_count")?.to_string(),
            canonical_count: counts.try_get::<i64, _>("canonical_count")?.to_string(),
            blocked_jobs: counts.try_get::<i64, _>("blocked_jobs")?.to_string(),
            failed_jobs: counts.try_get::<i64, _>("failed_jobs")?.to_string(),
            uncertain_actions: counts.try_get::<i64, _>("uncertain_actions")?.to_string(),
        };
        let bytes = serde_json::to_vec(&(instance_id, area, &observation))
            .map_err(|_| StoreError::Corrupt)?;
        let evidence: EvidenceDigest = Sha256::digest(bytes).into();
        sqlx::query("INSERT INTO supervisor_actions(id,area,source_event,kind,policy_revision,source_digest) VALUES($1,$2,$1,'health_check',$3,$4)")
            .bind(action_id).bind(area_name(area)).bind(policy.revision).bind(evidence.to_vec()).execute(&mut *tx).await?;
        journal(
            &mut tx,
            area,
            policy.revision,
            Some(action_id),
            None,
            "observed",
            Some(evidence),
        )
        .await?;
        for state in ["proposed", "authorized", "queued"] {
            sqlx::query("UPDATE supervisor_actions SET state=$2 WHERE id=$1")
                .bind(action_id)
                .bind(state)
                .execute(&mut *tx)
                .await?;
            journal(
                &mut tx,
                area,
                policy.revision,
                Some(action_id),
                None,
                state,
                Some(evidence),
            )
            .await?;
        }
        // The worker identity/fence exists only during this DB transaction; no
        // application caller obtains a claim usable for any external operation.
        sqlx::query("UPDATE supervisor_actions SET state='running',worker=$2,fence=1,lease_until=clock_timestamp()+interval '120 seconds' WHERE id=$1")
            .bind(action_id).bind(instance_id).execute(&mut *tx).await?;
        journal(
            &mut tx,
            area,
            policy.revision,
            Some(action_id),
            None,
            "claimed",
            None,
        )
        .await?;
        sqlx::query("INSERT INTO supervisor_health_observations(action_id,instance_id,area,policy_revision,observed_at,next_due_at,project_count,canonical_count,blocked_jobs,failed_jobs,uncertain_actions) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
            .bind(action_id).bind(instance_id).bind(area_name(area)).bind(policy.revision).bind(now).bind(next_due_at)
            .bind(counts.try_get::<i64,_>("project_count")?).bind(counts.try_get::<i64,_>("canonical_count")?)
            .bind(counts.try_get::<i64,_>("blocked_jobs")?).bind(counts.try_get::<i64,_>("failed_jobs")?).bind(counts.try_get::<i64,_>("uncertain_actions")?)
            .execute(&mut *tx).await?;
        let row = lock_action(&mut tx, area, action_id).await?;
        if db_now(&mut tx).await? >= row.try_get::<DateTime<Utc>, _>("lease_until")? {
            return Err(StoreError::Fenced);
        }
        actions::finish(&mut tx, &row, KnownOutcome::Verified, evidence).await?;
        sqlx::query("UPDATE supervisor_health_schedule SET next_due_at=$2 WHERE instance_id=$1")
            .bind(instance_id)
            .bind(next_due_at)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(observation))
    }
}

pub(crate) async fn latest(
    conn: &mut PgConnection,
    instance: Uuid,
) -> Result<Option<SupervisorHealthObservation>> {
    let row = sqlx::query("SELECT * FROM supervisor_health_observations WHERE instance_id=$1 ORDER BY observed_at DESC LIMIT 1")
        .bind(instance).fetch_optional(conn).await?;
    row.map(|row| {
        Ok(SupervisorHealthObservation {
            action_id: row.try_get("action_id")?,
            policy_revision: row.try_get("policy_revision")?,
            observed_at: row.try_get("observed_at")?,
            next_due_at: row.try_get("next_due_at")?,
            project_count: row.try_get::<i64, _>("project_count")?.to_string(),
            canonical_count: row.try_get::<i64, _>("canonical_count")?.to_string(),
            blocked_jobs: row.try_get::<i64, _>("blocked_jobs")?.to_string(),
            failed_jobs: row.try_get::<i64, _>("failed_jobs")?.to_string(),
            uncertain_actions: row.try_get::<i64, _>("uncertain_actions")?.to_string(),
        })
    })
    .transpose()
}
