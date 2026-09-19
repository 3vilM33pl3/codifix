//! Private exact-input intents. The trusted HTTP boundary supplies the actor,
//! validates the pinned source profile and commits before acknowledging enqueue.
//! No question body is part of the operational journal or retrieval corpus.
use super::*;
use mem_record::{AuthRole, Permission, PermissionSet, SupervisorQuestionStatus};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionActor {
    pub principal_id: Uuid,
    pub token_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionSubmission {
    pub request_id: Uuid,
    pub project_id: Uuid,
    pub instance_id: Uuid,
    pub profile_digest: EvidenceDigest,
    pub request_digest: EvidenceDigest,
    pub policy_revision: Uuid,
    pub actor: QuestionActor,
    pub question: String,
}

#[derive(Clone)]
pub struct QueuedQuestion {
    pub submission: QuestionSubmission,
    pub attempt_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Canonical fixed-field serialization, domain separated from file/model hashes.
pub fn terms_digest(value: &QuestionSubmission) -> Result<EvidenceDigest> {
    let bytes = serde_json::to_vec(value).map_err(|_| StoreError::InvalidInput)?;
    let mut hash = Sha256::new();
    hash.update(b"memory-workshop-question-v1\0");
    hash.update(bytes);
    Ok(hash.finalize().into())
}

pub fn validate_question(question: &str) -> Result<()> {
    if question.trim().is_empty()
        || question.len() > 4096
        || question
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
    {
        return Err(StoreError::InvalidInput);
    }
    Ok(())
}

fn permissions(role: Option<String>, value: Option<serde_json::Value>) -> Result<PermissionSet> {
    if let Some(value) = value {
        let values: Vec<Permission> =
            serde_json::from_value(value).map_err(|_| StoreError::Binding)?;
        return Ok(PermissionSet::from_permissions(&values));
    }
    role.map(|role| {
        serde_json::from_value::<AuthRole>(serde_json::Value::String(role))
            .map(AuthRole::permissions)
            .map_err(|_| StoreError::Binding)
    })
    .unwrap_or(Ok(PermissionSet::EMPTY))
}

/// Narrow queued authority: a persisted credential plus current direct database
/// grants. Group-mapping-only permissions are not sufficient. OIDC global roles
/// are ignored, matching the service authorization rule. No configuration lookup.
pub async fn authorize_requester_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &QuestionActor,
    project: Uuid,
) -> Result<()> {
    async {
        non_nil(actor.principal_id)?;
        non_nil(project)?;
        if actor.token_id.is_some() == actor.session_id.is_some() {
            return Err(StoreError::Binding);
        }
        let principal = sqlx::query("SELECT kind,global_role,global_permissions_json FROM auth_principals WHERE id=$1 AND disabled_at IS NULL FOR SHARE")
            .bind(actor.principal_id).fetch_optional(&mut **tx).await?.ok_or(StoreError::Binding)?;
        let members = sqlx::query("SELECT role,permissions_json FROM auth_project_memberships WHERE principal_id=$1 AND project_id=$2 FOR SHARE")
            .bind(actor.principal_id).bind(project).fetch_all(&mut **tx).await?;
        let current = if let Some(token) = actor.token_id {
            non_nil(token)?;
            sqlx::query("SELECT id FROM auth_service_tokens WHERE id=$1 AND principal_id=$2 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>clock_timestamp()) FOR SHARE")
                .bind(token).bind(actor.principal_id).fetch_optional(&mut **tx).await?.is_some()
        } else if let Some(session) = actor.session_id {
            non_nil(session)?;
            sqlx::query("SELECT id FROM auth_web_sessions WHERE id=$1 AND principal_id=$2 AND revoked_at IS NULL AND expires_at>clock_timestamp() FOR SHARE")
                .bind(session).bind(actor.principal_id).fetch_optional(&mut **tx).await?.is_some()
        } else { false };
        if !current { return Err(StoreError::Binding); }
        let role = if principal.try_get::<String,_>("kind")? == "human_oidc" { None }
            else { principal.try_get("global_role")? };
        let mut allowed = permissions(role, principal.try_get("global_permissions_json")?)?;
        for member in members {
            allowed = allowed.union(permissions(Some(member.try_get("role")?), member.try_get("permissions_json")?)?);
        }
        if [Permission::MemoryRead, Permission::LoopsRun, Permission::BundleExport].into_iter().any(|p| !allowed.contains(p)) {
            return Err(StoreError::Binding);
        }
        Ok(())
    }.await.map_err(sanitize)
}

fn decode(row: PgRow) -> Result<QueuedQuestion> {
    fn digest(row: &PgRow, name: &str) -> Result<EvidenceDigest> {
        row.try_get::<Vec<u8>, _>(name)?
            .try_into()
            .map_err(|_| StoreError::Corrupt)
    }
    Ok(QueuedQuestion {
        submission: QuestionSubmission {
            request_id: row.try_get("request_id")?,
            project_id: row.try_get("project_id")?,
            instance_id: row.try_get("instance_id")?,
            profile_digest: digest(&row, "profile_digest")?,
            request_digest: digest(&row, "request_digest")?,
            policy_revision: row.try_get("policy_revision")?,
            actor: QuestionActor {
                principal_id: row.try_get("principal_id")?,
                token_id: row.try_get("token_id")?,
                session_id: row.try_get("session_id")?,
            },
            question: row.try_get("question")?,
        },
        attempt_id: row.try_get("attempt_id")?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
    })
}

pub async fn read_tx(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    id: Uuid,
) -> Result<Option<QueuedQuestion>> {
    async {
        sqlx::query("SELECT * FROM supervisor_questions WHERE project_id=$1 AND request_id=$2")
            .bind(project)
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?
            .map(decode)
            .transpose()
    }
    .await
    .map_err(sanitize)
}

/// Caller retains current actor, exact source and policy locks through commit.
/// This insert IS the outbox; subsequent action creation is idempotent by ID.
pub async fn enqueue_tx(
    tx: &mut Transaction<'_, Postgres>,
    input: &QuestionSubmission,
) -> Result<QueuedQuestion> {
    async {
        for id in [input.request_id,input.project_id,input.instance_id,input.policy_revision] { non_nil(id)?; }
        validate_question(&input.question)?;
        authorize_requester_tx(tx,&input.actor,input.project_id).await?;
        // A fixed transaction lock serializes this tiny bounded intake only.
        // Workers never take it, so it cannot invert requester/source locks.
        sqlx::query("SELECT pg_advisory_xact_lock(1937076338,60)").execute(&mut **tx).await?;
        if let Some(existing) = read_tx(tx,input.project_id,input.request_id).await? {
            let mut replay = input.clone();
            // A newly bootstrapped session for the SAME actor may reconcile a
            // prior receipt; it never replaces the original credential binding.
            replay.actor = existing.submission.actor.clone();
            if existing.submission != replay || input.actor.principal_id != existing.submission.actor.principal_id {
                return Err(StoreError::Conflict);
            }
            return Ok(existing);
        }
        let occupied: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM supervisor_actions WHERE id=$1) OR EXISTS(SELECT 1 FROM supervisor_questions WHERE request_id=$1)")
            .bind(input.request_id).fetch_one(&mut **tx).await?;
        if occupied { return Err(StoreError::Conflict); }
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM supervisor_questions q LEFT JOIN supervisor_actions a ON a.id=q.request_id WHERE q.instance_id=$1 AND q.project_id=$2 AND q.expires_at>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM supervisor_question_denials d WHERE d.request_id=q.request_id) AND (a.id IS NULL OR a.state IN ('observed','proposed','authorized','queued','running'))")
            .bind(input.instance_id).bind(input.project_id).fetch_one(&mut **tx).await?;
        if count>=8 { return Err(StoreError::QueueFull); }
        let row=sqlx::query("INSERT INTO supervisor_questions(request_id,project_id,instance_id,profile_digest,request_digest,policy_revision,principal_id,token_id,session_id,question,attempt_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING *")
            .bind(input.request_id).bind(input.project_id).bind(input.instance_id).bind(input.profile_digest.to_vec()).bind(input.request_digest.to_vec()).bind(input.policy_revision)
            .bind(input.actor.principal_id).bind(input.actor.token_id).bind(input.actor.session_id).bind(&input.question).bind(Uuid::new_v4()).fetch_one(&mut **tx).await?;
        // Expiry of a blocked credential after an intake-lock wait is denial.
        authorize_requester_tx(tx,&input.actor,input.project_id).await?;
        decode(row)
    }.await.map_err(sanitize)
}

pub async fn authorize_question_tx(
    tx: &mut Transaction<'_, Postgres>,
    question: &QueuedQuestion,
) -> Result<()> {
    async {
        let input = &question.submission;
        authorize_requester_tx(tx, &input.actor, input.project_id).await?;
        let current = read_tx(tx, input.project_id, input.request_id)
            .await?
            .ok_or(StoreError::Binding)?;
        let denied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM supervisor_question_denials WHERE request_id=$1)",
        )
        .bind(input.request_id)
        .fetch_one(&mut **tx)
        .await?;
        if current.submission != *input
            || current.attempt_id != question.attempt_id
            || current.expires_at != question.expires_at
            || db_now(tx).await? >= current.expires_at
            || denied
        {
            return Err(StoreError::Binding);
        }
        Ok(())
    }
    .await
    .map_err(sanitize)
}

pub async fn list_tx(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    principal: Uuid,
    after: Option<Uuid>,
) -> Result<Vec<QueuedQuestion>> {
    async {
        sqlx::query("SELECT * FROM supervisor_questions WHERE project_id=$1 AND principal_id=$2 AND ($3::uuid IS NULL OR request_id>$3) ORDER BY request_id LIMIT 51")
            .bind(project).bind(principal).bind(after).fetch_all(&mut **tx).await?.into_iter().map(decode).collect::<Result<Vec<_>>>()
    }.await.map_err(sanitize)
}

pub async fn status_tx(
    tx: &mut Transaction<'_, Postgres>,
    question: &QueuedQuestion,
) -> Result<SupervisorQuestionStatus> {
    async {
        let id=question.submission.request_id;
        let row=sqlx::query("SELECT (SELECT state FROM supervisor_actions WHERE id=$1) AS state,EXISTS(SELECT 1 FROM supervisor_question_denials WHERE request_id=$1) AS denied,EXISTS(SELECT 1 FROM supervisor_proposals WHERE action_id=$1) AS proposal")
            .bind(id).fetch_one(&mut **tx).await?;
        let state:Option<String>=row.try_get("state")?;
        let proposal_available:bool=row.try_get("proposal")?;
        let state=match state.as_deref() {
            Some("verified") if proposal_available => "answered".to_owned(),
            Some("failed"|"denied"|"expired"|"cancelled"|"uncertain")=>state.ok_or(StoreError::Corrupt)?,
            _ if row.try_get::<bool,_>("denied")? => "denied".into(),
            // Do not misreport an admitted generation as harmless expiry.
            Some("running")=>"running".into(),
            _ if db_now(tx).await? >= question.expires_at=>"expired".into(),
            _=>"queued".into(),
        };
        Ok(SupervisorQuestionStatus {request_id:id,action_id:id,state,expires_at:question.expires_at,proposal_available})
    }.await.map_err(sanitize)
}

pub async fn next(
    pool: &PgPool,
    project: Uuid,
    profile_digest: EvidenceDigest,
) -> Result<Option<QueuedQuestion>> {
    async {
        sqlx::query("SELECT q.* FROM supervisor_questions q LEFT JOIN supervisor_actions a ON a.id=q.request_id WHERE q.project_id=$1 AND q.profile_digest=$2 AND q.expires_at>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM supervisor_question_denials d WHERE d.request_id=q.request_id) AND (a.id IS NULL OR a.state IN ('observed','proposed','authorized','queued')) ORDER BY q.created_at,q.request_id LIMIT 1")
            .bind(project).bind(profile_digest.to_vec()).fetch_optional(pool).await?.map(decode).transpose()
    }.await.map_err(sanitize)
}

pub async fn deny(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query(
        "INSERT INTO supervisor_question_denials(request_id) VALUES($1) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|_| StoreError::PrivateStorage)
}

fn sanitize(error: StoreError) -> StoreError {
    match error {
        StoreError::Database(_) => StoreError::PrivateStorage,
        other => other,
    }
}
