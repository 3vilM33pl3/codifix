// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_supervisor::{DAILY_AREA_MODEL_CALLS, DAILY_AREA_USD_MICROS, DEFAULT_MODEL};

#[derive(Debug, Clone, Serialize)]
pub struct PolicyRecord {
    pub area: Area,
    pub revision: Uuid,
    pub enabled: bool,
    pub revoked: bool,
    pub daily_limit: UsdMicros,
    pub daily_calls: u32,
}
impl PolicyRecord {
    pub fn model(&self) -> &'static str {
        DEFAULT_MODEL
    }
    pub(crate) fn validate(&self, revision: Uuid) -> Result<()> {
        if self.revoked {
            return Err(PolicyError::Revoked.into());
        }
        if !self.enabled {
            return Err(PolicyError::Disabled.into());
        }
        if self.revision != revision {
            return Err(PolicyError::StaleRevision.into());
        }
        Ok(())
    }
}

impl SupervisorStore {
    /// Caller owns authentication, request-id reconciliation and protected audit
    /// in this same transaction. No bootstrap, limit change, or model consent.
    pub async fn set_enabled_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        instance: Uuid,
        area: Area,
        revision: Uuid,
        expected_enabled: bool,
        enabled: bool,
    ) -> Result<()> {
        non_nil(instance)?;
        non_nil(revision)?;
        if expected_enabled == enabled {
            return Err(StoreError::InvalidInput);
        }
        sqlx::query("SELECT area FROM supervisor_authorities WHERE area=$1 FOR UPDATE")
            .bind(area_name(area))
            .fetch_one(&mut **tx)
            .await?;
        let binding = contexts::read_binding(tx)
            .await?
            .ok_or(StoreError::Binding)?;
        if binding.instance_id != instance || binding.area != area {
            return Err(StoreError::Binding);
        }
        let policy = read_policy(tx, area).await?;
        if policy.revision != revision {
            return Err(PolicyError::StaleRevision.into());
        }
        if policy.revoked {
            return Err(PolicyError::Revoked.into());
        }
        if policy.enabled != expected_enabled {
            return Err(StoreError::Conflict);
        }
        sqlx::query("UPDATE supervisor_policies SET enabled=$3 WHERE area=$1 AND revision=$2")
            .bind(area_name(area))
            .bind(revision)
            .bind(enabled)
            .execute(&mut **tx)
            .await?;
        journal(
            tx,
            area,
            revision,
            None,
            None,
            if enabled {
                "policy_enabled"
            } else {
                "policy_paused"
            },
            None,
        )
        .await?;
        Ok(())
    }
    /// Explicit operator rotation: old revision is permanently revoked, new
    /// revision starts disabled. Existing/revoked revisions cannot be reinstalled.
    pub async fn install_policy(&self, area: Area, revision: Uuid) -> Result<PolicyRecord> {
        non_nil(revision)?;
        let mut tx = self.pool.begin().await?;
        let current: Option<Uuid> = sqlx::query_scalar(
            "SELECT current_revision FROM supervisor_authorities WHERE area=$1 FOR UPDATE",
        )
        .bind(area_name(area))
        .fetch_one(&mut *tx)
        .await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM supervisor_policies WHERE area=$1 AND revision=$2)",
        )
        .bind(area_name(area))
        .bind(revision)
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            if current != Some(revision) {
                return Err(StoreError::Conflict);
            }
            let policy = read_policy(&mut tx, area).await?;
            tx.commit().await?;
            return Ok(policy);
        }
        if let Some(old) = current {
            sqlx::query("UPDATE supervisor_policies SET revoked=true,enabled=false WHERE area=$1 AND revision=$2")
                .bind(area_name(area)).bind(old).execute(&mut *tx).await?;
            journal(&mut tx, area, old, None, None, "policy_revoked", None).await?;
        }
        sqlx::query("INSERT INTO supervisor_policies(area,revision,daily_usd_micros,daily_calls) VALUES($1,$2,$3,$4)")
            .bind(area_name(area)).bind(revision).bind(DAILY_AREA_USD_MICROS as i64).bind(DAILY_AREA_MODEL_CALLS as i32).execute(&mut *tx).await?;
        sqlx::query("UPDATE supervisor_authorities SET current_revision=$2 WHERE area=$1")
            .bind(area_name(area))
            .bind(revision)
            .execute(&mut *tx)
            .await?;
        journal(
            &mut tx,
            area,
            revision,
            None,
            None,
            "policy_installed",
            None,
        )
        .await?;
        let policy = read_policy(&mut tx, area).await?;
        tx.commit().await?;
        Ok(policy)
    }

    pub async fn policy(&self, area: Area) -> Result<PolicyRecord> {
        let mut tx = self.pool.begin().await?;
        let policy = lock_policy(&mut tx, area).await?;
        tx.commit().await?;
        Ok(policy)
    }

    pub async fn set_enabled(&self, area: Area, revision: Uuid, enabled: bool) -> Result<()> {
        self.change_policy(area, revision, Change::Enabled(enabled))
            .await
    }
    pub async fn revoke_policy(&self, area: Area, revision: Uuid) -> Result<()> {
        self.change_policy(area, revision, Change::Revoke).await
    }
    /// Same-revision limits only decrease. Rotation does not reset usage.
    pub async fn reduce_limits(
        &self,
        area: Area,
        revision: Uuid,
        amount: UsdMicros,
        calls: u32,
    ) -> Result<()> {
        self.change_policy(area, revision, Change::Reduce(amount, calls))
            .await
    }
    async fn change_policy(&self, area: Area, revision: Uuid, change: Change) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT area FROM supervisor_authorities WHERE area=$1 FOR UPDATE")
            .bind(area_name(area))
            .fetch_one(&mut *tx)
            .await?;
        let policy = read_policy(&mut tx, area).await?;
        if revision != policy.revision {
            return Err(PolicyError::StaleRevision.into());
        }
        let event = match change {
            Change::Enabled(enabled) => {
                if enabled && policy.revoked {
                    return Err(PolicyError::Revoked.into());
                }
                sqlx::query(
                    "UPDATE supervisor_policies SET enabled=$3 WHERE area=$1 AND revision=$2",
                )
                .bind(area_name(area))
                .bind(revision)
                .bind(enabled)
                .execute(&mut *tx)
                .await?;
                if enabled {
                    "policy_enabled"
                } else {
                    "policy_paused"
                }
            }
            Change::Revoke => {
                sqlx::query("UPDATE supervisor_policies SET enabled=false,revoked=true WHERE area=$1 AND revision=$2")
                    .bind(area_name(area)).bind(revision).execute(&mut *tx).await?;
                "policy_revoked"
            }
            Change::Reduce(amount, calls) => {
                if amount > policy.daily_limit || calls > policy.daily_calls {
                    return Err(StoreError::InvalidInput);
                }
                sqlx::query("UPDATE supervisor_policies SET daily_usd_micros=$3,daily_calls=$4 WHERE area=$1 AND revision=$2")
                    .bind(area_name(area)).bind(revision).bind(amount.0 as i64).bind(calls as i32).execute(&mut *tx).await?;
                "policy_reduced"
            }
        };
        journal(&mut tx, area, revision, None, None, event, None).await?;
        tx.commit().await?;
        Ok(())
    }
}
enum Change {
    Enabled(bool),
    Revoke,
    Reduce(UsdMicros, u32),
}

pub(crate) async fn lock_policy(conn: &mut PgConnection, area: Area) -> Result<PolicyRecord> {
    // Shared lock permits unrelated actions/days, exclusive changes linearize
    // against every admission, begin, claim and publication transaction.
    sqlx::query("SELECT area FROM supervisor_authorities WHERE area=$1 FOR SHARE")
        .bind(area_name(area))
        .fetch_one(&mut *conn)
        .await?;
    read_policy(conn, area).await
}
pub(crate) async fn read_policy(conn: &mut PgConnection, area: Area) -> Result<PolicyRecord> {
    let row = sqlx::query("SELECT p.* FROM supervisor_policies p JOIN supervisor_authorities a ON (p.area=a.area AND p.revision=a.current_revision) WHERE a.area=$1")
        .bind(area_name(area)).fetch_optional(conn).await?.ok_or(StoreError::NotFound)?;
    Ok(PolicyRecord {
        area,
        revision: row.try_get("revision")?,
        enabled: row.try_get("enabled")?,
        revoked: row.try_get("revoked")?,
        daily_limit: UsdMicros(row.try_get::<i64, _>("daily_usd_micros")? as u64),
        daily_calls: row.try_get::<i32, _>("daily_calls")? as u32,
    })
}
