// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_record::{
    SupervisorActionCount, SupervisorBudgetSummary, SupervisorJournalEntry,
    SupervisorPolicySummary, SupervisorSnapshot,
};

impl SupervisorStore {
    /// One consistent read-only snapshot. Caller must authorize instance-wide
    /// inspection; project access alone is insufficient. No lazy bootstrap,
    /// action claims, budget reservations or provider invocation occurs here.
    pub async fn inspect(
        &self,
        instance_id: Uuid,
        expected_area: Area,
        after: i64,
        limit: u32,
    ) -> Result<SupervisorSnapshot> {
        non_nil(instance_id)?;
        if after < 0 || !(1..=100).contains(&limit) {
            return Err(StoreError::InvalidInput);
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        // Establish the snapshot before selecting the wall clock/accounting day.
        let binding = contexts::read_binding(&mut tx).await?;
        if binding
            .as_ref()
            .is_some_and(|b| b.instance_id != instance_id || b.area != expected_area)
        {
            return Err(StoreError::Binding);
        }
        let observed_at = db_now(&mut tx).await?;
        let mut snapshot = SupervisorSnapshot {
            instance_id,
            area: binding.as_ref().map(|b| b.area),
            observed_at,
            policy: None,
            budget: None,
            last_health_observation: None,
            action_counts: vec![],
            journal: vec![],
            next_after: after.to_string(),
            has_more: false,
        };
        if let Some(binding) = binding {
            snapshot.last_health_observation = health::latest(&mut tx, instance_id).await?;
            let area = binding.area;
            match policy::read_policy(&mut tx, area).await {
                Ok(policy) => {
                    snapshot.policy = Some(SupervisorPolicySummary {
                        revision: policy.revision,
                        enabled: policy.enabled,
                        revoked: policy.revoked,
                        model: policy.model().into(),
                        daily_usd_micros: policy.daily_limit.0.to_string(),
                        daily_calls: policy.daily_calls,
                    })
                }
                Err(StoreError::NotFound) => {}
                Err(error) => return Err(error),
            }
            let day = observed_at.date_naive();
            let totals = attempts::totals(&mut tx, area, day).await?;
            snapshot.budget = Some(SupervisorBudgetSummary {
                day,
                charged_usd_micros: totals.charged_usd_micros.to_string(),
                reserved_usd_micros: totals.reserved_usd_micros.to_string(),
                calls: totals.calls,
            });
            let counts = sqlx::query("SELECT state,count(*) AS count FROM supervisor_actions WHERE area=$1 GROUP BY state ORDER BY state")
                .bind(area_name(area)).fetch_all(&mut *tx).await?;
            for row in counts {
                snapshot.action_counts.push(SupervisorActionCount {
                    state: serde_json::from_value(serde_json::Value::String(row.try_get("state")?))
                        .map_err(|_| StoreError::Corrupt)?,
                    count: row.try_get::<i64, _>("count")?.to_string(),
                });
            }
            let mut rows = sqlx::query("SELECT sequence,action_id,attempt_id,policy_revision,event,created_at FROM supervisor_journal WHERE area=$1 AND sequence>$2 ORDER BY sequence LIMIT $3")
                .bind(area_name(area)).bind(after).bind(i64::from(limit)+1)
                .fetch_all(&mut *tx).await?;
            snapshot.has_more = rows.len() > limit as usize;
            rows.truncate(limit as usize);
            for row in rows {
                snapshot.journal.push(SupervisorJournalEntry {
                    sequence: row.try_get::<i64, _>("sequence")?.to_string(),
                    action_id: row.try_get("action_id")?,
                    attempt_id: row.try_get("attempt_id")?,
                    policy_revision: row.try_get("policy_revision")?,
                    event: serde_json::from_value(serde_json::Value::String(row.try_get("event")?))
                        .map_err(|_| StoreError::Corrupt)?,
                    recorded_at: row.try_get("created_at")?,
                });
            }
            if let Some(last) = snapshot.journal.last() {
                snapshot.next_after = last.sequence.clone();
            }
        }
        tx.commit().await?;
        Ok(snapshot)
    }
}
