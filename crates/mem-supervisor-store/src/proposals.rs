// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private typed suggestions, separated from operational metadata and authority.
use super::*;
use mem_supervisor::{
    action::{ActionKind, KnownOutcome},
    budget::AttemptState,
    proposal::Proposal,
};
use sha2::{Digest, Sha256};

/// Intentionally no Debug. Reads are exact-scope trusted in-process operations,
/// not proof of current service access, factual truth or execution approval.
#[derive(Serialize)]
pub struct PrivateProposal {
    pub area: Area,
    pub project_id: Uuid,
    pub action_id: Uuid,
    pub attempt_id: Uuid,
    pub policy_revision: Uuid,
    pub fence: i64,
    pub request_digest: EvidenceDigest,
    pub proposal_digest: EvidenceDigest,
    pub proposal: Proposal,
    pub evidence_digests: Vec<EvidenceDigest>,
    pub created_at: DateTime<Utc>,
}

impl SupervisorStore {
    /// First publication commits the complete private body and verified outcome
    /// atomically. Exact retries only return existing evidence, even if the old
    /// claim expired or policy was revoked; they never restore authority.
    /// Caller must authorize the project and revalidate current source eligibility.
    pub async fn publish_proposal(
        &self,
        claim: &LeaseClaim,
        attempt_id: &Uuid,
        project_id: Uuid,
        proposal: &Proposal,
        evidence_digests: &[EvidenceDigest],
    ) -> Result<PrivateProposal> {
        self.publish_private(claim, *attempt_id, project_id, proposal, evidence_digests)
            .await
            .map_err(sanitize)
    }

    async fn publish_private(
        &self,
        claim: &LeaseClaim,
        attempt_id: Uuid,
        project_id: Uuid,
        proposal: &Proposal,
        evidence_digests: &[EvidenceDigest],
    ) -> Result<PrivateProposal> {
        non_nil(project_id)?;
        non_nil(attempt_id)?;
        proposal
            .validate(evidence_digests.len())
            .map_err(|_| StoreError::InvalidProposal)?;
        let mut tx = self.pool.begin().await?;
        let policy = policy::lock_policy(&mut tx, claim.area).await?;
        let action = lock_action(&mut tx, claim.area, claim.action_id).await?;
        allowed_kind(&action)?;
        let attempt = attempts::read_attempt(&mut tx, attempt_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if attempt.record.area != claim.area
            || attempt.record.action_id != claim.action_id
            || attempt.record.policy_revision != action.try_get::<Uuid, _>("policy_revision")?
            || attempt.fence != claim.fence
        {
            return Err(StoreError::Fenced);
        }
        if !matches!(attempt.record.state, AttemptState::Settled { .. }) {
            return Err(StoreError::Unsettled);
        }
        let digest = proposal_digest(
            &Bindings {
                area: claim.area,
                action_id: claim.action_id,
                attempt_id,
                policy_revision: attempt.record.policy_revision,
                fence: claim.fence,
                project_id,
                request_digest: attempt.record.request_digest,
            },
            proposal,
            evidence_digests,
        )?;
        // Search by action first so a wrong-project retry cannot try a second
        // insert or relabel already published private evidence.
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM supervisor_proposals WHERE action_id=$1)",
        )
        .bind(claim.action_id)
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            let existing = read_private(&mut tx, claim.area, project_id, claim.action_id)
                .await?
                .ok_or(StoreError::Conflict)?;
            if existing.proposal_digest != digest
                || existing.attempt_id != attempt_id
                || existing.fence != claim.fence
            {
                return Err(StoreError::Conflict);
            }
            tx.commit().await?;
            return Ok(existing);
        }
        policy.validate(attempt.record.policy_revision)?;
        check_lease(&mut tx, &action, claim).await?;
        let context = contexts::validate_current(&mut tx, &action).await?;
        if !context.matches_result(project_id, attempt.record.request_digest, evidence_digests) {
            return Err(StoreError::Conflict);
        }
        let present: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM projects WHERE id=$1 FOR KEY SHARE")
                .bind(project_id)
                .fetch_optional(&mut *tx)
                .await?;
        if present.is_none() {
            return Err(StoreError::NotFound);
        }
        // Json serializes only the neutral typed contract, never arbitrary fields.
        let digests: Vec<Vec<u8>> = evidence_digests
            .iter()
            .map(|digest| digest.to_vec())
            .collect();
        sqlx::query("INSERT INTO supervisor_proposals(action_id,area,attempt_id,policy_revision,fence,project_id,request_digest,proposal_digest,body,evidence_digests) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(claim.action_id).bind(area_name(claim.area)).bind(attempt_id).bind(attempt.record.policy_revision)
            .bind(claim.fence).bind(project_id).bind(attempt.record.request_digest.to_vec()).bind(digest.to_vec())
            .bind(sqlx::types::Json(proposal)).bind(digests).execute(&mut *tx).await?;
        actions::finish(&mut tx, &action, KnownOutcome::Verified, digest).await?;
        // finish can wait for the area journal head: lease must still be live.
        check_lease(&mut tx, &action, claim).await?;
        let stored = read_private(&mut tx, claim.area, project_id, claim.action_id)
            .await?
            .ok_or(StoreError::Corrupt)?;
        tx.commit().await?;
        Ok(stored)
    }

    /// Exact scope only. Retained project UUIDs survive project deletion; this
    /// does not confer access to a deleted/current project or its source data.
    /// The service must independently authorize every private read and retry.
    pub async fn private_proposal(
        &self,
        area: Area,
        project_id: Uuid,
        action_id: Uuid,
    ) -> Result<PrivateProposal> {
        async {
            non_nil(project_id)?;
            non_nil(action_id)?;
            let mut conn = self.pool.acquire().await?;
            read_private(&mut conn, area, project_id, action_id)
                .await?
                .ok_or(StoreError::NotFound)
        }
        .await
        .map_err(sanitize)
    }
}

// Database errors may include failing-row DETAIL containing private JSON.
// Never retain them as source/debug/error context at this API boundary.
fn sanitize(error: StoreError) -> StoreError {
    match error {
        StoreError::Database(_) => StoreError::PrivateStorage,
        other => other,
    }
}

#[derive(Serialize)]
struct Bindings {
    area: Area,
    action_id: Uuid,
    attempt_id: Uuid,
    policy_revision: Uuid,
    fence: i64,
    project_id: Uuid,
    request_digest: EvidenceDigest,
}

fn proposal_digest(
    bindings: &Bindings,
    proposal: &Proposal,
    evidence: &[EvidenceDigest],
) -> Result<EvidenceDigest> {
    let bytes = serde_json::to_vec(&(bindings, proposal, evidence))
        .map_err(|_| StoreError::InvalidProposal)?;
    let mut hasher = Sha256::new();
    hasher.update(b"memory-supervisor-private-proposal-v1\0");
    hasher.update(bytes);
    Ok(hasher.finalize().into())
}

pub(crate) fn allowed_kind(action: &PgRow) -> Result<()> {
    match actions::parse_kind(&action.try_get::<String, _>("kind")?)? {
        ActionKind::HealthCheck | ActionKind::InstallMaintenance => {
            Err(StoreError::InvalidProposal)
        }
        _ => Ok(()),
    }
}

async fn read_private(
    conn: &mut PgConnection,
    area: Area,
    project: Uuid,
    action_id: Uuid,
) -> Result<Option<PrivateProposal>> {
    let Some(row) = sqlx::query(
        "SELECT * FROM supervisor_proposals WHERE area=$1 AND project_id=$2 AND action_id=$3",
    )
    .bind(area_name(area))
    .bind(project)
    .bind(action_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    let proposal: sqlx::types::Json<Proposal> = row.try_get("body")?;
    let evidence: Vec<Vec<u8>> = row.try_get("evidence_digests")?;
    let evidence: Vec<EvidenceDigest> = evidence
        .into_iter()
        .map(|digest| digest.try_into().map_err(|_| StoreError::Corrupt))
        .collect::<Result<_>>()?;
    proposal
        .validate(evidence.len())
        .map_err(|_| StoreError::Corrupt)?;
    let bindings = Bindings {
        area,
        action_id,
        project_id: project,
        attempt_id: row.try_get("attempt_id")?,
        policy_revision: row.try_get("policy_revision")?,
        fence: row.try_get("fence")?,
        request_digest: row
            .try_get::<Vec<u8>, _>("request_digest")?
            .try_into()
            .map_err(|_| StoreError::Corrupt)?,
    };
    let stored_digest: EvidenceDigest = row
        .try_get::<Vec<u8>, _>("proposal_digest")?
        .try_into()
        .map_err(|_| StoreError::Corrupt)?;
    if proposal_digest(&bindings, &proposal, &evidence)? != stored_digest {
        return Err(StoreError::Corrupt);
    }
    let action = sqlx::query("SELECT * FROM supervisor_actions WHERE area=$1 AND id=$2")
        .bind(area_name(area))
        .bind(action_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(StoreError::Corrupt)?;
    allowed_kind(&action).map_err(|_| StoreError::Corrupt)?;
    if action.try_get::<String, _>("state")? != "verified"
        || action.try_get::<Uuid, _>("policy_revision")? != bindings.policy_revision
        || Some(action.try_get::<i64, _>("fence")?) != bindings.fence.checked_add(1)
    {
        return Err(StoreError::Corrupt);
    }
    let attempt = attempts::read_attempt(conn, bindings.attempt_id)
        .await?
        .ok_or(StoreError::Corrupt)?;
    if attempt.record.area != area
        || attempt.record.action_id != action_id
        || attempt.record.policy_revision != bindings.policy_revision
        || attempt.fence != bindings.fence
        || attempt.record.request_digest != bindings.request_digest
        || !matches!(attempt.record.state, AttemptState::Settled { .. })
    {
        return Err(StoreError::Corrupt);
    }
    let result = sqlx::query(
        "SELECT outcome,evidence_digest FROM supervisor_action_results WHERE action_id=$1",
    )
    .bind(action_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(StoreError::Corrupt)?;
    if result.try_get::<String, _>("outcome")? != "verified"
        || result.try_get::<Vec<u8>, _>("evidence_digest")? != stored_digest
    {
        return Err(StoreError::Corrupt);
    }
    // Old immutable 0054 history remains readable; all new publication paths
    // require 0055 context, and retained contexts must agree with their result.
    if let Some(context) = contexts::read_context(conn, area, action_id).await?
        && (context.policy_revision() != bindings.policy_revision
            || !context.matches_result(project, bindings.request_digest, &evidence))
    {
        return Err(StoreError::Corrupt);
    }
    Ok(Some(PrivateProposal {
        area,
        project_id: project,
        action_id,
        attempt_id: bindings.attempt_id,
        policy_revision: bindings.policy_revision,
        fence: bindings.fence,
        request_digest: bindings.request_digest,
        proposal_digest: stored_digest,
        proposal: proposal.0,
        evidence_digests: evidence,
        created_at: row.try_get("created_at")?,
    }))
}
