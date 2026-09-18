// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_supervisor::{
    DEFAULT_MODEL,
    budget::{AttemptState, BudgetTotals, ProviderRequest, Settlement},
    usage::ProviderUsage,
};

#[derive(Debug, Clone, Serialize)]
pub struct AttemptRecord {
    pub id: Uuid,
    pub area: Area,
    pub action_id: Uuid,
    pub policy_revision: Uuid,
    pub day: NaiveDate,
    pub request_digest: EvidenceDigest,
    pub worst_case_cost: UsdMicros,
    pub state: AttemptState,
}

#[derive(Debug, Clone, Serialize)]
pub enum Reservation {
    Reserved(AttemptRecord),
    Existing(AttemptRecord),
}

/// Issued only after the begin transaction commits, never reissued on retries.
/// The sender consumes this value exactly once and binds the complete wire body
/// to request_digest. It must bound its deadline by expires_at. This crate does
/// not perform network I/O or make a permit an external revocation mechanism.
pub struct SendPermit {
    attempt_id: Uuid,
    action_id: Uuid,
    area: Area,
    request_digest: EvidenceDigest,
    worst_case_cost: UsdMicros,
    admitted_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}
impl SendPermit {
    pub fn attempt_id(&self) -> Uuid {
        self.attempt_id
    }
    pub fn action_id(&self) -> Uuid {
        self.action_id
    }
    pub fn area(&self) -> Area {
        self.area
    }
    pub fn model(&self) -> &'static str {
        DEFAULT_MODEL
    }
    pub fn request_digest(&self) -> EvidenceDigest {
        self.request_digest
    }
    pub fn worst_case_cost(&self) -> UsdMicros {
        self.worst_case_cost
    }
    pub fn admitted_at(&self) -> DateTime<Utc> {
        self.admitted_at
    }
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

impl SupervisorStore {
    /// A caller must establish a conservative bound using an exact approved
    /// model, input count, output cap and pricing. Storage cannot prove pricing.
    pub async fn reserve(
        &self,
        claim: &LeaseClaim,
        request: &ProviderRequest,
        request_digest: EvidenceDigest,
    ) -> Result<Reservation> {
        let id = opaque(&request.id)?;
        if opaque(&request.action_id)? != claim.action_id {
            return Err(StoreError::Conflict);
        }
        let revision = opaque(&request.policy_revision)?;
        if request.model.as_str() != DEFAULT_MODEL {
            return Err(PolicyError::ModelNotApproved.into());
        }
        if request.worst_case_cost.0 == 0 {
            return Err(BudgetError::MissingCostBound.into());
        }
        if request.worst_case_cost.0 > mem_supervisor::DAILY_AREA_USD_MICROS {
            return Err(BudgetError::CostLimit.into());
        }
        let mut tx = self.pool.begin().await?;
        let policy = policy::lock_policy(&mut tx, claim.area).await?;
        policy.validate(revision)?;
        let row = lock_action(&mut tx, claim.area, claim.action_id).await?;
        if row.try_get::<Uuid, _>("policy_revision")? != revision {
            return Err(PolicyError::StaleRevision.into());
        }
        check_lease(&mut tx, &row, claim).await?;
        let context = contexts::validate_current(&mut tx, &row).await?;
        if context.request_digest() != request_digest {
            return Err(StoreError::Conflict);
        }
        let day = current_day(&mut tx).await?;
        lock_day(&mut tx, claim.area, day).await?;
        if let Some(existing) = read_attempt(&mut tx, id).await? {
            if existing.record.area != claim.area
                || existing.record.action_id != claim.action_id
                || existing.record.policy_revision != revision
                || existing.record.request_digest != request_digest
                || existing.record.worst_case_cost != request.worst_case_cost
            {
                return Err(StoreError::Conflict);
            }
            tx.commit().await?;
            return Ok(Reservation::Existing(existing.record));
        }
        let totals = totals(&mut tx, claim.area, day).await?;
        if totals.calls >= policy.daily_calls {
            return Err(BudgetError::CallLimit.into());
        }
        if totals.charged_usd_micros
            + totals.reserved_usd_micros
            + u128::from(request.worst_case_cost.0)
            > u128::from(policy.daily_limit.0)
        {
            return Err(BudgetError::CostLimit.into());
        }
        // Identity conflicts in another area are rejected by the unique key;
        // duplicate same-area requests serialize on the action/day locks.
        sqlx::query("INSERT INTO supervisor_attempts(id,area,action_id,policy_revision,fence,day,model,request_digest,worst_usd_micros) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(id).bind(area_name(claim.area)).bind(claim.action_id).bind(revision).bind(claim.fence).bind(day).bind(DEFAULT_MODEL).bind(request_digest.to_vec()).bind(request.worst_case_cost.0 as i64).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO supervisor_attempt_states(attempt_id) VALUES($1)")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        journal(
            &mut tx,
            claim.area,
            revision,
            Some(claim.action_id),
            Some(id),
            "reserved",
            Some(request_digest),
        )
        .await?;
        check_lease(&mut tx, &row, claim).await?;
        let record = read_attempt(&mut tx, id)
            .await?
            .ok_or(StoreError::Corrupt)?
            .record;
        tx.commit().await?;
        Ok(Reservation::Reserved(record))
    }

    pub async fn begin(&self, claim: &LeaseClaim, attempt_id: Uuid) -> Result<SendPermit> {
        let mut tx = self.pool.begin().await?;
        let policy = policy::lock_policy(&mut tx, claim.area).await?;
        let row = lock_action(&mut tx, claim.area, claim.action_id).await?;
        policy.validate(row.try_get("policy_revision")?)?;
        check_lease(&mut tx, &row, claim).await?;
        let attempt = read_attempt(&mut tx, attempt_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if attempt.record.area != claim.area
            || attempt.record.action_id != claim.action_id
            || attempt.fence != claim.fence
        {
            return Err(StoreError::Fenced);
        }
        policy.validate(attempt.record.policy_revision)?;
        let context = contexts::validate_current(&mut tx, &row).await?;
        if context.request_digest() != attempt.record.request_digest {
            return Err(StoreError::Conflict);
        }
        if attempt.record.day != current_day(&mut tx).await? {
            return Err(BudgetError::WrongDay.into());
        }
        lock_day(&mut tx, claim.area, attempt.record.day).await?;
        let totals = totals(&mut tx, claim.area, attempt.record.day).await?;
        if totals.calls > policy.daily_calls
            || totals.charged_usd_micros + totals.reserved_usd_micros
                > u128::from(policy.daily_limit.0)
        {
            return Err(BudgetError::ReducedLimit.into());
        }
        if attempt.record.state != AttemptState::Reserved {
            return Err(StoreError::Transition);
        }
        set_state(&mut tx, attempt_id, "in_flight").await?;
        journal(
            &mut tx,
            claim.area,
            attempt.record.policy_revision,
            Some(claim.action_id),
            Some(attempt_id),
            "begun",
            None,
        )
        .await?;
        // Recheck after any journal contention; no stale permit from waiting on
        // a lock longer than the lease. Rollback preserves reserved state.
        check_lease(&mut tx, &row, claim).await?;
        let admitted_at = db_now(&mut tx).await?;
        if admitted_at >= claim.expires_at {
            return Err(StoreError::Fenced);
        }
        let permit = SendPermit {
            attempt_id,
            action_id: claim.action_id,
            area: claim.area,
            request_digest: attempt.record.request_digest,
            worst_case_cost: attempt.record.worst_case_cost,
            admitted_at,
            expires_at: claim.expires_at,
        };
        tx.commit().await?;
        Ok(permit)
    }

    pub async fn release_before_send(
        &self,
        claim: &LeaseClaim,
        attempt_id: Uuid,
        evidence: EvidenceDigest,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let row = lock_action(&mut tx, claim.area, claim.action_id).await?;
        check_lease(&mut tx, &row, claim).await?;
        let attempt = read_attempt(&mut tx, attempt_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if attempt.record.action_id != claim.action_id || attempt.fence != claim.fence {
            return Err(StoreError::Fenced);
        }
        lock_day(&mut tx, claim.area, attempt.record.day).await?;
        if attempt.record.state == AttemptState::ReleasedBeforeSend {
            return Ok(());
        }
        if attempt.record.state != AttemptState::Reserved {
            return Err(StoreError::Transition);
        }
        set_state(&mut tx, attempt_id, "released_before_send").await?;
        journal(
            &mut tx,
            claim.area,
            attempt.record.policy_revision,
            Some(claim.action_id),
            Some(attempt_id),
            "released_before_send",
            Some(evidence),
        )
        .await?;
        check_lease(&mut tx, &row, claim).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Transport ambiguity is accounting, not result publication. No lease or
    /// active policy is required; this never frees the reservation.
    pub async fn mark_uncertain(&self, attempt_id: Uuid) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let original = read_attempt(&mut tx, attempt_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        lock_action(&mut tx, original.record.area, original.record.action_id).await?;
        let attempt = read_attempt(&mut tx, attempt_id)
            .await?
            .ok_or(StoreError::Corrupt)?;
        if attempt.record.state == AttemptState::Uncertain {
            return Ok(());
        }
        if attempt.record.state != AttemptState::InFlight {
            return Err(StoreError::Transition);
        }
        set_state(&mut tx, attempt_id, "uncertain").await?;
        journal(
            &mut tx,
            attempt.record.area,
            attempt.record.policy_revision,
            Some(attempt.record.action_id),
            Some(attempt_id),
            "uncertain",
            None,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Authoritative usage settlement is immutable and idempotent, even after
    /// revocation/day rollover. Over-bound real charges are recorded, not hidden.
    pub async fn settle(
        &self,
        attempt_id: Uuid,
        actual_cost: UsdMicros,
        evidence: EvidenceDigest,
    ) -> Result<Settlement> {
        self.settle_measured(attempt_id, actual_cost, evidence, None)
            .await
    }

    /// Counters and cost commit together, even if authority was revoked after
    /// dispatch. Exact retries cannot change or retroactively invent counters.
    pub async fn settle_with_usage(
        &self,
        attempt_id: Uuid,
        actual_cost: UsdMicros,
        evidence: EvidenceDigest,
        usage: ProviderUsage,
    ) -> Result<Settlement> {
        if !usage.validate() {
            return Err(StoreError::InvalidInput);
        }
        self.settle_measured(attempt_id, actual_cost, evidence, Some(usage))
            .await
    }

    async fn settle_measured(
        &self,
        attempt_id: Uuid,
        actual_cost: UsdMicros,
        evidence: EvidenceDigest,
        usage: Option<ProviderUsage>,
    ) -> Result<Settlement> {
        let mut tx = self.pool.begin().await?;
        let original = read_attempt(&mut tx, attempt_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let exceeded = actual_cost > original.record.worst_case_cost;
        if exceeded {
            // Acquire before action/day locks, the same order as admission.
            // This fences new sends while the real over-bound charge is stored.
            sqlx::query("SELECT area FROM supervisor_authorities WHERE area=$1 FOR UPDATE")
                .bind(area_name(original.record.area))
                .fetch_one(&mut *tx)
                .await?;
        }
        lock_action(&mut tx, original.record.area, original.record.action_id).await?;
        lock_day(&mut tx, original.record.area, original.record.day).await?;
        let attempt = read_attempt(&mut tx, attempt_id)
            .await?
            .ok_or(StoreError::Corrupt)?;
        if let AttemptState::Settled { actual_cost: old } = attempt.record.state {
            if old != actual_cost
                || attempt.settlement_digest.as_deref() != Some(evidence.as_slice())
                || usage != attempt.usage
            {
                return Err(StoreError::Conflict);
            }
            return Ok(Settlement::AlreadySettled);
        }
        if !matches!(
            attempt.record.state,
            AttemptState::InFlight | AttemptState::Uncertain
        ) {
            return Err(StoreError::Transition);
        }
        let counters = usage.map(|u| u.counters().map(|n| n.to_string()).to_vec());
        sqlx::query("INSERT INTO supervisor_settlements(attempt_id,actual_usd_micros,evidence_digest,token_usage) VALUES($1,$2::text::numeric,$3,$4::text[]::numeric[])")
            .bind(attempt_id).bind(actual_cost.0.to_string()).bind(evidence.to_vec()).bind(counters).execute(&mut *tx).await?;
        set_state(&mut tx, attempt_id, "settled").await?;
        journal(
            &mut tx,
            attempt.record.area,
            attempt.record.policy_revision,
            Some(attempt.record.action_id),
            Some(attempt_id),
            if exceeded {
                "settled_over_bound"
            } else {
                "settled"
            },
            Some(evidence),
        )
        .await?;
        if exceeded {
            let revision: Option<Uuid> = sqlx::query_scalar(
                "SELECT current_revision FROM supervisor_authorities WHERE area=$1",
            )
            .bind(area_name(attempt.record.area))
            .fetch_one(&mut *tx)
            .await?;
            if let Some(revision) = revision {
                sqlx::query("UPDATE supervisor_policies SET enabled=false,revoked=true WHERE area=$1 AND revision=$2")
                    .bind(area_name(attempt.record.area)).bind(revision).execute(&mut *tx).await?;
                journal(
                    &mut tx,
                    attempt.record.area,
                    revision,
                    None,
                    Some(attempt_id),
                    "policy_revoked",
                    Some(evidence),
                )
                .await?;
            }
        }
        tx.commit().await?;
        Ok(if exceeded {
            Settlement::ExceededReservation
        } else {
            Settlement::Settled
        })
    }

    pub async fn attempt(&self, id: Uuid) -> Result<AttemptRecord> {
        let mut conn = self.pool.acquire().await?;
        Ok(read_attempt(&mut conn, id)
            .await?
            .ok_or(StoreError::NotFound)?
            .record)
    }

    /// Trusted exact-attempt read; service callers must enforce project/area
    /// scope. None means unknown legacy usage or an unsettled attempt, NOT zero.
    pub async fn settled_usage(&self, id: Uuid) -> Result<Option<ProviderUsage>> {
        let mut conn = self.pool.acquire().await?;
        Ok(read_attempt(&mut conn, id)
            .await?
            .ok_or(StoreError::NotFound)?
            .usage)
    }
    pub async fn budget(&self, area: Area, day: NaiveDate) -> Result<BudgetTotals> {
        let mut conn = self.pool.acquire().await?;
        totals(&mut conn, area, day).await
    }
    pub async fn current_budget(&self, area: Area) -> Result<(NaiveDate, BudgetTotals)> {
        let mut conn = self.pool.acquire().await?;
        let day = current_day(&mut conn).await?;
        Ok((day, totals(&mut conn, area, day).await?))
    }
}

async fn lock_day(conn: &mut PgConnection, area: Area, day: NaiveDate) -> Result<()> {
    sqlx::query(
        "INSERT INTO supervisor_budget_days(area,day) VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(area_name(area))
    .bind(day)
    .execute(&mut *conn)
    .await?;
    sqlx::query("SELECT area FROM supervisor_budget_days WHERE area=$1 AND day=$2 FOR UPDATE")
        .bind(area_name(area))
        .bind(day)
        .fetch_one(conn)
        .await?;
    Ok(())
}
pub(crate) async fn totals(
    conn: &mut PgConnection,
    area: Area,
    day: NaiveDate,
) -> Result<BudgetTotals> {
    let row = sqlx::query("SELECT coalesce(sum(CASE WHEN s.state='settled' THEN u.actual_usd_micros ELSE 0 END),0)::text AS charged,coalesce(sum(CASE WHEN s.state IN ('reserved','in_flight','uncertain') THEN a.worst_usd_micros ELSE 0 END),0)::text AS reserved,count(*) FILTER (WHERE s.state<>'released_before_send') AS calls FROM supervisor_attempts a JOIN supervisor_attempt_states s ON s.attempt_id=a.id LEFT JOIN supervisor_settlements u ON u.attempt_id=a.id WHERE a.area=$1 AND a.day=$2")
        .bind(area_name(area)).bind(day).fetch_one(conn).await?;
    Ok(BudgetTotals {
        charged_usd_micros: row
            .try_get::<String, _>("charged")?
            .parse()
            .map_err(|_| StoreError::Corrupt)?,
        reserved_usd_micros: row
            .try_get::<String, _>("reserved")?
            .parse()
            .map_err(|_| StoreError::Corrupt)?,
        calls: u32::try_from(row.try_get::<i64, _>("calls")?).map_err(|_| StoreError::Corrupt)?,
    })
}
async fn set_state(conn: &mut PgConnection, id: Uuid, state: &str) -> Result<()> {
    sqlx::query("UPDATE supervisor_attempt_states SET state=$2,updated_at=clock_timestamp() WHERE attempt_id=$1")
        .bind(id).bind(state).execute(conn).await?;
    Ok(())
}
pub(crate) struct StoredAttempt {
    pub(crate) record: AttemptRecord,
    pub(crate) fence: i64,
    settlement_digest: Option<Vec<u8>>,
    usage: Option<ProviderUsage>,
}
pub(crate) async fn read_attempt(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<StoredAttempt>> {
    let Some(row) = sqlx::query("SELECT a.*,s.state,u.actual_usd_micros::text AS actual,u.evidence_digest AS settlement_digest,u.token_usage::text[] AS token_usage FROM supervisor_attempts a JOIN supervisor_attempt_states s ON s.attempt_id=a.id LEFT JOIN supervisor_settlements u ON u.attempt_id=a.id WHERE a.id=$1")
        .bind(id).fetch_optional(conn).await? else { return Ok(None); };
    let state = match row.try_get::<String, _>("state")?.as_str() {
        "reserved" => AttemptState::Reserved,
        "in_flight" => AttemptState::InFlight,
        "uncertain" => AttemptState::Uncertain,
        "released_before_send" => AttemptState::ReleasedBeforeSend,
        "settled" => AttemptState::Settled {
            actual_cost: UsdMicros(
                row.try_get::<String, _>("actual")?
                    .parse()
                    .map_err(|_| StoreError::Corrupt)?,
            ),
        },
        _ => return Err(StoreError::Corrupt),
    };
    Ok(Some(StoredAttempt {
        usage: row
            .try_get::<Option<Vec<String>>, _>("token_usage")?
            .map(|values| {
                let numbers = values
                    .iter()
                    .map(|v| v.parse::<u64>().map_err(|_| StoreError::Corrupt))
                    .collect::<Result<Vec<_>>>()?;
                ProviderUsage::from_counters(numbers.try_into().map_err(|_| StoreError::Corrupt)?)
                    .ok_or(StoreError::Corrupt)
            })
            .transpose()?,
        fence: row.try_get("fence")?,
        settlement_digest: row.try_get("settlement_digest")?,
        record: AttemptRecord {
            id,
            area: parse_area(&row.try_get::<String, _>("area")?)?,
            action_id: row.try_get("action_id")?,
            policy_revision: row.try_get("policy_revision")?,
            day: row.try_get("day")?,
            request_digest: row
                .try_get::<Vec<u8>, _>("request_digest")?
                .try_into()
                .map_err(|_| StoreError::Corrupt)?,
            worst_case_cost: UsdMicros(row.try_get::<i64, _>("worst_usd_micros")? as u64),
            state,
        },
    }))
}
