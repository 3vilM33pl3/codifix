// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local instance identity and immutable model-input scope, not authorization.
use super::*;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
pub struct InstanceBinding {
    pub instance_id: Uuid,
    pub area: Area,
    pub created_at: DateTime<Utc>,
}

/// Opaque, non-deserializable metadata. Every use rechecks persisted binding,
/// current policy/lease and project existence. This is not a bearer capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelWorkContext {
    instance_id: Uuid,
    area: Area,
    action_id: Uuid,
    policy_revision: Uuid,
    project_id: Uuid,
    request_digest: EvidenceDigest,
    evidence_digests: Vec<EvidenceDigest>,
    context_digest: EvidenceDigest,
}
impl ModelWorkContext {
    pub fn instance_id(&self) -> Uuid {
        self.instance_id
    }
    pub fn area(&self) -> Area {
        self.area
    }
    pub fn action_id(&self) -> Uuid {
        self.action_id
    }
    pub fn policy_revision(&self) -> Uuid {
        self.policy_revision
    }
    pub fn project_id(&self) -> Uuid {
        self.project_id
    }
    pub fn request_digest(&self) -> EvidenceDigest {
        self.request_digest
    }
    pub fn evidence_digests(&self) -> &[EvidenceDigest] {
        &self.evidence_digests
    }
    pub fn context_digest(&self) -> EvidenceDigest {
        self.context_digest
    }

    pub(crate) fn matches_result(
        &self,
        project: Uuid,
        request: EvidenceDigest,
        evidence: &[EvidenceDigest],
    ) -> bool {
        self.project_id == project
            && self.request_digest == request
            && self.evidence_digests == evidence
    }
}

impl SupervisorStore {
    /// Trusted explicit bootstrap. Exactly one immutable identity/area per DB;
    /// no implicit policy enablement or claim of uniqueness across DB clones.
    pub async fn bind_instance(&self, instance_id: Uuid, area: Area) -> Result<InstanceBinding> {
        non_nil(instance_id)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO supervisor_instance_binding(instance_id,area) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(instance_id).bind(area_name(area)).execute(&mut *tx).await?;
        let binding = read_binding(&mut tx).await?.ok_or(StoreError::Corrupt)?;
        if binding.instance_id != instance_id || binding.area != area {
            return Err(StoreError::Conflict);
        }
        tx.commit().await?;
        Ok(binding)
    }

    pub async fn instance_binding(&self) -> Result<Option<InstanceBinding>> {
        let mut conn = self.pool.acquire().await?;
        read_binding(&mut conn).await
    }

    /// Bind before token counting. Caller separately authorizes the exact
    /// project/source disclosure; existence and hashes cannot grant permission.
    /// No fence is stored here: a valid renewal retains context but all uses
    /// still require the newly current claim. Legacy attempts cannot be backfilled.
    pub async fn bind_model_work(
        &self,
        claim: &LeaseClaim,
        instance_id: Uuid,
        project_id: Uuid,
        request_digest: EvidenceDigest,
        evidence_digests: &[EvidenceDigest],
    ) -> Result<ModelWorkContext> {
        self.bind_work(
            claim,
            instance_id,
            project_id,
            request_digest,
            evidence_digests,
        )
        .await
        .map_err(sanitize)
    }

    async fn bind_work(
        &self,
        claim: &LeaseClaim,
        instance_id: Uuid,
        project_id: Uuid,
        request_digest: EvidenceDigest,
        evidence_digests: &[EvidenceDigest],
    ) -> Result<ModelWorkContext> {
        non_nil(instance_id)?;
        non_nil(project_id)?;
        if evidence_digests.len() > mem_supervisor::proposal::MAX_EVIDENCE {
            return Err(StoreError::InvalidInput);
        }
        let mut tx = self.pool.begin().await?;
        let policy = policy::lock_policy(&mut tx, claim.area).await?;
        let action = lock_action(&mut tx, claim.area, claim.action_id).await?;
        let revision = action.try_get("policy_revision")?;
        policy.validate(revision)?;
        check_lease(&mut tx, &action, claim).await?;
        proposals::allowed_kind(&action)?;
        let binding = read_binding(&mut tx).await?.ok_or(StoreError::Binding)?;
        if binding.instance_id != instance_id || binding.area != claim.area {
            return Err(StoreError::Binding);
        }
        require_project(&mut tx, project_id).await?;
        let mut context = ModelWorkContext {
            instance_id,
            area: claim.area,
            action_id: claim.action_id,
            policy_revision: revision,
            project_id,
            request_digest,
            evidence_digests: evidence_digests.to_vec(),
            context_digest: [0; 32],
        };
        context.context_digest = digest(&context)?;
        if let Some(existing) = read_context(&mut tx, claim.area, claim.action_id).await? {
            if existing != context {
                return Err(StoreError::Conflict);
            }
        } else {
            let attempted: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM supervisor_attempts WHERE action_id=$1)",
            )
            .bind(claim.action_id)
            .fetch_one(&mut *tx)
            .await?;
            if attempted {
                return Err(StoreError::Binding);
            }
            let evidence: Vec<Vec<u8>> = evidence_digests
                .iter()
                .map(|value| value.to_vec())
                .collect();
            sqlx::query("INSERT INTO supervisor_model_contexts(action_id,instance_id,area,policy_revision,project_id,request_digest,evidence_digests,context_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(claim.action_id).bind(instance_id).bind(area_name(claim.area)).bind(revision).bind(project_id)
                .bind(request_digest.to_vec()).bind(evidence).bind(context.context_digest.to_vec()).execute(&mut *tx).await?;
            journal(
                &mut tx,
                claim.area,
                revision,
                Some(claim.action_id),
                None,
                "model_context_bound",
                Some(context.context_digest),
            )
            .await?;
        }
        check_lease(&mut tx, &action, claim).await?;
        tx.commit().await?;
        Ok(context)
    }

    /// Call before and after token counting and immediately before admission.
    /// Permission/source eligibility must be rechecked by the trusted caller.
    pub async fn check_model_work(
        &self,
        claim: &LeaseClaim,
        context: &ModelWorkContext,
    ) -> Result<DateTime<Utc>> {
        async {
            let mut tx = self.pool.begin().await?;
            let policy = policy::lock_policy(&mut tx, claim.area).await?;
            let action = lock_action(&mut tx, claim.area, claim.action_id).await?;
            policy.validate(action.try_get("policy_revision")?)?;
            check_lease(&mut tx, &action, claim).await?;
            let stored = validate_current(&mut tx, &action).await?;
            if &stored != context {
                return Err(StoreError::Binding);
            }
            check_lease(&mut tx, &action, claim).await?;
            let now = db_now(&mut tx).await?;
            if now >= claim.expires_at {
                return Err(StoreError::Fenced);
            }
            tx.commit().await?;
            Ok(now)
        }
        .await
        .map_err(sanitize)
    }
}

fn sanitize(error: StoreError) -> StoreError {
    match error {
        StoreError::Database(_) => StoreError::PrivateStorage,
        other => other,
    }
}
pub(crate) async fn read_binding(conn: &mut PgConnection) -> Result<Option<InstanceBinding>> {
    let Some(row) = sqlx::query(
        "SELECT instance_id,area,created_at FROM supervisor_instance_binding WHERE singleton",
    )
    .fetch_optional(conn)
    .await?
    else {
        return Ok(None);
    };
    Ok(Some(InstanceBinding {
        instance_id: row.try_get("instance_id")?,
        area: parse_area(&row.try_get::<String, _>("area")?)?,
        created_at: row.try_get("created_at")?,
    }))
}
async fn require_project(conn: &mut PgConnection, id: Uuid) -> Result<()> {
    let found: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM projects WHERE id=$1 FOR KEY SHARE")
            .bind(id)
            .fetch_optional(conn)
            .await?;
    if found.is_none() {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

pub(crate) async fn validate_current(
    conn: &mut PgConnection,
    action: &PgRow,
) -> Result<ModelWorkContext> {
    proposals::allowed_kind(action)?;
    let area = parse_area(&action.try_get::<String, _>("area")?)?;
    let context = read_context(conn, area, action.try_get("id")?)
        .await?
        .ok_or(StoreError::Binding)?;
    if context.policy_revision != action.try_get::<Uuid, _>("policy_revision")? {
        return Err(StoreError::Binding);
    }
    require_project(conn, context.project_id).await?;
    Ok(context)
}

pub(crate) async fn read_context(
    conn: &mut PgConnection,
    area: Area,
    action_id: Uuid,
) -> Result<Option<ModelWorkContext>> {
    let Some(row) =
        sqlx::query("SELECT * FROM supervisor_model_contexts WHERE area=$1 AND action_id=$2")
            .bind(area_name(area))
            .bind(action_id)
            .fetch_optional(&mut *conn)
            .await?
    else {
        return Ok(None);
    };
    let evidence: Vec<Vec<u8>> = row.try_get("evidence_digests")?;
    let evidence_digests = evidence
        .into_iter()
        .map(|value| value.try_into().map_err(|_| StoreError::Corrupt))
        .collect::<Result<Vec<EvidenceDigest>>>()?;
    if evidence_digests.len() > mem_supervisor::proposal::MAX_EVIDENCE {
        return Err(StoreError::Corrupt);
    }
    let context = ModelWorkContext {
        instance_id: row.try_get("instance_id")?,
        area,
        action_id,
        policy_revision: row.try_get("policy_revision")?,
        project_id: row.try_get("project_id")?,
        request_digest: row
            .try_get::<Vec<u8>, _>("request_digest")?
            .try_into()
            .map_err(|_| StoreError::Corrupt)?,
        evidence_digests,
        context_digest: row
            .try_get::<Vec<u8>, _>("context_digest")?
            .try_into()
            .map_err(|_| StoreError::Corrupt)?,
    };
    if context.context_digest != digest(&context)? {
        return Err(StoreError::Corrupt);
    }
    let binding = read_binding(conn).await?.ok_or(StoreError::Binding)?;
    if binding.area != context.area || binding.instance_id != context.instance_id {
        return Err(StoreError::Binding);
    }
    Ok(Some(context))
}
fn digest(context: &ModelWorkContext) -> Result<EvidenceDigest> {
    let bytes = serde_json::to_vec(&(
        context.instance_id,
        context.area,
        context.action_id,
        context.policy_revision,
        context.project_id,
        context.request_digest,
        &context.evidence_digests,
    ))
    .map_err(|_| StoreError::Corrupt)?;
    let mut hash = Sha256::new();
    hash.update(b"memory-supervisor-model-context-v1\0");
    hash.update(bytes);
    Ok(hash.finalize().into())
}
