// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_supervisor::action::{ActionKey, ActionKind, KnownOutcome, StopReason};

#[derive(Debug, Clone)]
pub struct ActionSubmission {
    pub id: Uuid,
    /// Source event and revision must be UUID-form StableIds, never prose.
    pub key: ActionKey,
    pub source_digest: EvidenceDigest,
}

#[derive(Debug, Clone, Copy)]
pub enum ReadyTransition {
    Propose,
    Authorize,
    Queue,
}

impl SupervisorStore {
    pub async fn submit(&self, action: &ActionSubmission) -> Result<ActionRecord> {
        non_nil(action.id)?;
        let event = opaque(&action.key.source_event)?;
        let revision = opaque(&action.key.policy_revision)?;
        let area = action.key.area;
        let mut tx = self.pool.begin().await?;
        let policy = policy::lock_policy(&mut tx, area).await?;
        let inserted = sqlx::query("INSERT INTO supervisor_actions(id,area,source_event,kind,policy_revision,source_digest) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING")
            .bind(action.id).bind(area_name(area)).bind(event).bind(kind_name(action.key.kind)).bind(revision).bind(action.source_digest.to_vec()).execute(&mut *tx).await?.rows_affected();
        let row = sqlx::query("SELECT * FROM supervisor_actions WHERE id=$1 OR (area=$2 AND source_event=$3 AND kind=$4 AND policy_revision=$5)")
            .bind(action.id).bind(area_name(area)).bind(event).bind(kind_name(action.key.kind)).bind(revision).fetch_one(&mut *tx).await?;
        if row.try_get::<Uuid, _>("id")? != action.id
            || row.try_get::<String, _>("area")? != area_name(area)
            || row.try_get::<Uuid, _>("source_event")? != event
            || row.try_get::<String, _>("kind")? != kind_name(action.key.kind)
            || row.try_get::<Uuid, _>("policy_revision")? != revision
            || row.try_get::<Vec<u8>, _>("source_digest")? != action.source_digest
        {
            return Err(StoreError::Conflict);
        }
        if inserted == 1 {
            if policy.revision != revision {
                return Err(PolicyError::StaleRevision.into());
            }
            journal(
                &mut tx,
                area,
                revision,
                Some(action.id),
                None,
                "observed",
                Some(action.source_digest),
            )
            .await?;
        }
        let record = action_record(&row)?;
        tx.commit().await?;
        Ok(record)
    }

    /// The embedding trusted authorization boundary must additionally validate
    /// action-specific permissions. This common policy check does not do so.
    pub async fn advance(
        &self,
        area: Area,
        id: Uuid,
        transition: ReadyTransition,
        evidence: EvidenceDigest,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let policy = policy::lock_policy(&mut tx, area).await?;
        let row = lock_action(&mut tx, area, id).await?;
        let revision = row.try_get("policy_revision")?;
        let (from, to) = match transition {
            ReadyTransition::Propose => ("observed", "proposed"),
            ReadyTransition::Authorize => ("proposed", "authorized"),
            ReadyTransition::Queue => ("authorized", "queued"),
        };
        if !matches!(transition, ReadyTransition::Propose) {
            policy.validate(revision)?;
        }
        if row.try_get::<String, _>("state")? != from {
            return Err(StoreError::Transition);
        }
        sqlx::query("UPDATE supervisor_actions SET state=$2 WHERE id=$1")
            .bind(id)
            .bind(to)
            .execute(&mut *tx)
            .await?;
        journal(&mut tx, area, revision, Some(id), None, to, Some(evidence)).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn claim(
        &self,
        area: Area,
        id: Uuid,
        worker: Uuid,
        ttl_seconds: u32,
    ) -> Result<LeaseClaim> {
        non_nil(worker)?;
        check_ttl(ttl_seconds)?;
        let mut tx = self.pool.begin().await?;
        let policy = policy::lock_policy(&mut tx, area).await?;
        let row = lock_action(&mut tx, area, id).await?;
        policy.validate(row.try_get("policy_revision")?)?;
        if row.try_get::<String, _>("state")? != "queued" {
            return Err(StoreError::Transition);
        }
        let row = sqlx::query("UPDATE supervisor_actions SET state='running',worker=$2,fence=fence+1,lease_until=clock_timestamp()+$3::bigint * INTERVAL '1 second' WHERE id=$1 RETURNING fence,lease_until")
            .bind(id).bind(worker).bind(i64::from(ttl_seconds)).fetch_one(&mut *tx).await?;
        let claim = LeaseClaim {
            area,
            action_id: id,
            worker,
            fence: row.try_get("fence")?,
            expires_at: row.try_get("lease_until")?,
        };
        journal(
            &mut tx,
            area,
            policy.revision,
            Some(id),
            None,
            "claimed",
            None,
        )
        .await?;
        if db_now(&mut tx).await? >= claim.expires_at {
            return Err(StoreError::Fenced);
        }
        tx.commit().await?;
        Ok(claim)
    }

    /// Renewal replaces the fence, invalidating the old claim and its unbegun
    /// reservations. Reserve only after renewal; never renew across a send.
    pub async fn renew(&self, claim: &LeaseClaim, ttl_seconds: u32) -> Result<LeaseClaim> {
        check_ttl(ttl_seconds)?;
        let mut tx = self.pool.begin().await?;
        let policy = policy::lock_policy(&mut tx, claim.area).await?;
        let row = lock_action(&mut tx, claim.area, claim.action_id).await?;
        policy.validate(row.try_get("policy_revision")?)?;
        check_lease(&mut tx, &row, claim).await?;
        let expiry = db_now(&mut tx).await? + chrono::Duration::seconds(i64::from(ttl_seconds));
        if expiry <= claim.expires_at {
            return Err(StoreError::InvalidInput);
        }
        let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM supervisor_attempts a JOIN supervisor_attempt_states s ON s.attempt_id=a.id WHERE a.action_id=$1 AND s.state IN ('reserved','in_flight','uncertain')")
            .bind(claim.action_id).fetch_one(&mut *tx).await?;
        if pending != 0 {
            return Err(StoreError::Unsettled);
        }
        let fence: i64 = sqlx::query_scalar("UPDATE supervisor_actions SET fence=fence+1,lease_until=$2 WHERE id=$1 RETURNING fence")
            .bind(claim.action_id).bind(expiry).fetch_one(&mut *tx).await?;
        journal(
            &mut tx,
            claim.area,
            policy.revision,
            Some(claim.action_id),
            None,
            "renewed",
            None,
        )
        .await?;
        check_lease(&mut tx, &row, claim).await?;
        tx.commit().await?;
        Ok(LeaseClaim {
            fence,
            expires_at: expiry,
            ..claim.clone()
        })
    }

    pub async fn publish(
        &self,
        claim: &LeaseClaim,
        outcome: KnownOutcome,
        evidence: EvidenceDigest,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let policy = policy::lock_policy(&mut tx, claim.area).await?;
        let row = lock_action(&mut tx, claim.area, claim.action_id).await?;
        policy.validate(row.try_get("policy_revision")?)?;
        check_lease(&mut tx, &row, claim).await?;
        finish(&mut tx, &row, outcome, evidence).await?;
        check_lease(&mut tx, &row, claim).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn stop(
        &self,
        area: Area,
        id: Uuid,
        reason: StopReason,
        evidence: EvidenceDigest,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let row = lock_action(&mut tx, area, id).await?;
        match row.try_get::<String, _>("state")?.as_str() {
            "running" => make_uncertain(&mut tx, &row, Some(evidence)).await?,
            "observed" | "proposed" | "authorized" | "queued" => {
                let state = match reason {
                    StopReason::Denied => "denied",
                    StopReason::Cancelled => "cancelled",
                    StopReason::Expired => "expired",
                };
                sqlx::query("UPDATE supervisor_actions SET state=$2,fence=fence+1 WHERE id=$1")
                    .bind(id)
                    .bind(state)
                    .execute(&mut *tx)
                    .await?;
                journal(
                    &mut tx,
                    area,
                    row.try_get("policy_revision")?,
                    Some(id),
                    None,
                    state,
                    Some(evidence),
                )
                .await?;
            }
            _ => return Err(StoreError::Transition),
        }
        tx.commit().await?;
        Ok(())
    }

    /// Explicit trusted reconciliation only. Uncertain actions cannot be queued
    /// or claimed again. Verified publication still requires current authority;
    /// failure accounting may close an action after revocation.
    pub async fn reconcile(
        &self,
        area: Area,
        id: Uuid,
        outcome: KnownOutcome,
        evidence: EvidenceDigest,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let policy = policy::lock_policy(&mut tx, area).await?;
        let row = lock_action(&mut tx, area, id).await?;
        if outcome == KnownOutcome::Verified {
            policy.validate(row.try_get("policy_revision")?)?;
        }
        if row.try_get::<String, _>("state")? != "uncertain" {
            return Err(StoreError::Transition);
        }
        finish(&mut tx, &row, outcome, evidence).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Recovery never requeues a possibly dispatched effect or frees its budget.
    pub async fn reap_expired(&self, area: Area, limit: u32) -> Result<u32> {
        if !(1..=500).contains(&limit) {
            return Err(StoreError::InvalidInput);
        }
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query("SELECT * FROM supervisor_actions WHERE area=$1 AND state='running' AND lease_until<=clock_timestamp() ORDER BY id LIMIT $2 FOR UPDATE SKIP LOCKED")
            .bind(area_name(area)).bind(i64::from(limit)).fetch_all(&mut *tx).await?;
        for row in &rows {
            make_uncertain(&mut tx, row, None).await?;
        }
        tx.commit().await?;
        Ok(rows.len() as u32)
    }
}

async fn make_uncertain(
    conn: &mut PgConnection,
    row: &PgRow,
    evidence: Option<EvidenceDigest>,
) -> Result<()> {
    let id = row.try_get("id")?;
    let area = parse_area(&row.try_get::<String, _>("area")?)?;
    let revision = row.try_get("policy_revision")?;
    let attempts: Vec<Uuid> = sqlx::query_scalar("UPDATE supervisor_attempt_states s SET state='uncertain',updated_at=clock_timestamp() FROM supervisor_attempts a WHERE a.id=s.attempt_id AND a.action_id=$1 AND s.state IN ('reserved','in_flight') RETURNING a.id")
        .bind(id).fetch_all(&mut *conn).await?;
    for attempt in attempts {
        journal(
            conn,
            area,
            revision,
            Some(id),
            Some(attempt),
            "uncertain",
            evidence,
        )
        .await?;
    }
    sqlx::query("UPDATE supervisor_actions SET state='uncertain',fence=fence+1,worker=NULL,lease_until=NULL WHERE id=$1").bind(id).execute(&mut *conn).await?;
    journal(conn, area, revision, Some(id), None, "uncertain", evidence).await
}
pub(crate) async fn finish(
    conn: &mut PgConnection,
    row: &PgRow,
    outcome: KnownOutcome,
    evidence: EvidenceDigest,
) -> Result<()> {
    let id: Uuid = row.try_get("id")?;
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM supervisor_attempts a JOIN supervisor_attempt_states s ON s.attempt_id=a.id WHERE a.action_id=$1 AND s.state NOT IN ('settled','released_before_send')")
        .bind(id).fetch_one(&mut *conn).await?;
    if pending != 0 {
        return Err(StoreError::Unsettled);
    }
    if outcome == KnownOutcome::Verified {
        let had_attempt: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM supervisor_attempts WHERE action_id=$1)",
        )
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
        if had_attempt {
            // A generic hash-only completion must not bypass atomic private
            // result persistence. publish_proposal inserts this row before us.
            let persisted: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM supervisor_proposals p JOIN supervisor_attempts a ON a.id=p.attempt_id JOIN supervisor_attempt_states s ON s.attempt_id=a.id WHERE p.action_id=$1 AND p.area=$2 AND p.policy_revision=$3 AND p.fence=$4 AND p.proposal_digest=$5 AND a.action_id=p.action_id AND a.area=p.area AND a.policy_revision=p.policy_revision AND a.fence=p.fence AND a.request_digest=p.request_digest AND s.state='settled')")
                .bind(id).bind(row.try_get::<String,_>("area")?).bind(row.try_get::<Uuid,_>("policy_revision")?)
                .bind(row.try_get::<i64,_>("fence")?).bind(evidence.to_vec()).fetch_one(&mut *conn).await?;
            if !persisted {
                return Err(StoreError::UnpersistedProposal);
            }
        }
    }
    let state = match outcome {
        KnownOutcome::Verified => "verified",
        KnownOutcome::Failed => "failed",
    };
    sqlx::query(
        "INSERT INTO supervisor_action_results(action_id,outcome,evidence_digest) VALUES($1,$2,$3)",
    )
    .bind(id)
    .bind(state)
    .bind(evidence.to_vec())
    .execute(&mut *conn)
    .await?;
    sqlx::query("UPDATE supervisor_actions SET state=$2,fence=fence+1,worker=NULL,lease_until=NULL WHERE id=$1")
        .bind(id).bind(state).execute(&mut *conn).await?;
    journal(
        conn,
        parse_area(&row.try_get::<String, _>("area")?)?,
        row.try_get("policy_revision")?,
        Some(id),
        None,
        state,
        Some(evidence),
    )
    .await
}
fn check_ttl(ttl: u32) -> Result<()> {
    if (1..=300).contains(&ttl) {
        Ok(())
    } else {
        Err(StoreError::InvalidInput)
    }
}
pub(crate) fn kind_name(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::HealthCheck => "health_check",
        ActionKind::StaleKnowledgeReview => "stale_knowledge_review",
        ActionKind::AskQuestion => "ask_question",
        ActionKind::ProposeCorrection => "propose_correction",
        ActionKind::ReviewRelease => "review_release",
        ActionKind::InstallMaintenance => "install_maintenance",
    }
}
pub(crate) fn parse_kind(kind: &str) -> Result<ActionKind> {
    match kind {
        "health_check" => Ok(ActionKind::HealthCheck),
        "stale_knowledge_review" => Ok(ActionKind::StaleKnowledgeReview),
        "ask_question" => Ok(ActionKind::AskQuestion),
        "propose_correction" => Ok(ActionKind::ProposeCorrection),
        "review_release" => Ok(ActionKind::ReviewRelease),
        "install_maintenance" => Ok(ActionKind::InstallMaintenance),
        _ => Err(StoreError::Corrupt),
    }
}
pub(crate) fn parse_state(state: &str) -> Result<ActionState> {
    match state {
        "observed" => Ok(ActionState::Observed),
        "proposed" => Ok(ActionState::Proposed),
        "authorized" => Ok(ActionState::Authorized),
        "queued" => Ok(ActionState::Queued),
        "running" => Ok(ActionState::Running),
        "verified" => Ok(ActionState::Verified),
        "denied" => Ok(ActionState::Denied),
        "failed" => Ok(ActionState::Failed),
        "expired" => Ok(ActionState::Expired),
        "cancelled" => Ok(ActionState::Cancelled),
        "uncertain" => Ok(ActionState::Uncertain),
        _ => Err(StoreError::Corrupt),
    }
}
