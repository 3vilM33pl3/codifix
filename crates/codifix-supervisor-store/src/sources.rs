// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exact local source-version bindings for a trusted, separately authorized
//! supervisor collection. No broad corpus search, raw captures, sessions, source
//! paths, credentials, automatic selection or implicit disclosure consent.
use super::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use std::collections::BTreeSet;

/// Explicit operator-configured local grant. Not a request body, bearer token,
/// inferred role or cross-instance publication permission. Persist it in the
/// approved deployment configuration; revoking its policy revision or principal
/// revokes it. The host must disable execution in read-only/evaluation mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCollectionGrant {
    pub instance_id: Uuid,
    pub project_id: Uuid,
    pub principal_id: Uuid,
    pub policy_revision: Uuid,
    pub destination: ModelDestination,
    pub expires_at: DateTime<Utc>,
    pub sources: Vec<SourceBinding>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelDestination {
    #[serde(rename = "openai")]
    OpenAi,
}

/// Initial Workshop-only deployment profile. Require an explicit minimal
/// permission override on a dedicated internal identity: never inherit a human,
/// global/group role, default operator preset or another project's grants.
/// Lock order: principal, memberships, sources, supervisor policy. The same
/// transaction may publish the result, so revocation cannot pass that commit.
pub async fn authorize_tx(
    tx: &mut Transaction<'_, Postgres>,
    grant: &LocalCollectionGrant,
) -> Result<Vec<String>> {
    async {
        for id in [grant.instance_id, grant.project_id, grant.principal_id, grant.policy_revision] { non_nil(id)?; }
        let current: Option<bool> = sqlx::query_scalar("SELECT kind='internal' AND disabled_at IS NULL AND global_role IS NULL AND (global_permissions_json IS NULL OR global_permissions_json='[]'::jsonb) AND groups_json='[]'::jsonb FROM auth_principals WHERE id=$1 FOR UPDATE")
            .bind(grant.principal_id).fetch_optional(&mut **tx).await?;
        if current != Some(true) { return Err(StoreError::Binding); }
        let rows = sqlx::query("SELECT project_id,role,permissions_json FROM auth_project_memberships WHERE principal_id=$1 ORDER BY project_id FOR SHARE")
            .bind(grant.principal_id).fetch_all(&mut **tx).await?;
        if rows.len()!=1 || rows[0].try_get::<Uuid,_>("project_id")? != grant.project_id { return Err(StoreError::Binding); }
        let role: String = rows[0].try_get("role")?;
        if !matches!(role.as_str(), "reader"|"writer"|"operator"|"admin") { return Err(StoreError::Binding); }
        let value: Option<serde_json::Value> = rows[0].try_get("permissions_json")?;
        let permissions: BTreeSet<codifix_record::Permission> = serde_json::from_value(value.ok_or(StoreError::Binding)?).map_err(|_| StoreError::Binding)?;
        let required = BTreeSet::from([codifix_record::Permission::MemoryRead, codifix_record::Permission::BundleExport, codifix_record::Permission::LoopsRun]);
        if permissions != required { return Err(StoreError::Binding); }
        let texts = validate_tx(tx, grant.project_id, &grant.sources).await?;
        let policy = policy::lock_policy(tx, Area::Workshop).await?;
        policy.validate(grant.policy_revision)?;
        let binding = contexts::read_binding(tx).await?.ok_or(StoreError::Binding)?;
        if binding.area != Area::Workshop || binding.instance_id != grant.instance_id || db_now(tx).await? >= grant.expires_at {
            return Err(StoreError::Binding);
        }
        Ok(texts)
    }.await.map_err(sanitize)
}

/// Metadata selected under standing or per-collection operator consent. A hash
/// is an integrity check, NOT an authorization token or credential scrubber.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceBinding {
    pub canonical_id: Uuid,
    pub version_id: Uuid,
    pub version: i32,
    pub evidence_digest: EvidenceDigest,
}

/// Sensitive exact input text. No Debug; never use in operational listings.
pub struct SourceProjection {
    pub binding: SourceBinding,
    pub text: String,
}

#[derive(Serialize)]
struct Evidence {
    project_id: Uuid,
    canonical_id: Uuid,
    version_id: Uuid,
    version: i32,
    authored_at: DateTime<Utc>,
    summary: String,
    text: String,
}

/// Call only after current exact-project access is locked. Acquires canonical
/// locks in stable order; keeps them until the caller's transaction finishes.
/// Use to prepare a bounded review pack, not to auto-approve it. Model input is
/// ONLY this typed projection, never a generic MemoryEntryResponse serialization.
pub async fn prepare_tx(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    canonical_ids: &[Uuid],
) -> Result<Vec<SourceProjection>> {
    async {
        non_nil(project)?;
        if canonical_ids.is_empty() || canonical_ids.len() > 8 {
            return Err(StoreError::InvalidInput);
        }
        let ordered: BTreeSet<_> = canonical_ids.iter().copied().collect();
        if ordered.len() != canonical_ids.len() || ordered.iter().any(Uuid::is_nil) {
            return Err(StoreError::InvalidInput);
        }
        for id in ordered {
            let status: Option<String> = sqlx::query_scalar("SELECT status FROM memory_canonical_state WHERE project_id=$1 AND canonical_id=$2 FOR SHARE")
                .bind(project).bind(id).fetch_optional(&mut **tx).await?;
            if status.as_deref() != Some("active") { return Err(StoreError::Binding); }
        }
        let mut result = Vec::with_capacity(canonical_ids.len());
        let mut bytes = 0;
        for canonical in canonical_ids {
            let row = sqlx::query("SELECT id,version_no,is_tombstone,canonical_text,summary,created_at FROM memory_entries WHERE project_id=$1 AND canonical_id=$2 ORDER BY version_no DESC LIMIT 1 FOR SHARE")
                .bind(project).bind(canonical).fetch_optional(&mut **tx).await?.ok_or(StoreError::Binding)?;
            if row.try_get::<bool,_>("is_tombstone")? { return Err(StoreError::Binding); }
            let evidence = Evidence {
                project_id: project, canonical_id: *canonical, version_id: row.try_get("id")?, version: row.try_get("version_no")?,
                authored_at: row.try_get("created_at")?, summary: row.try_get("summary")?, text: row.try_get("canonical_text")?,
            };
            if evidence.version <= 0 || evidence.text.trim().is_empty() { return Err(StoreError::Binding); }
            let text = serde_json::to_string(&evidence).map_err(|_| StoreError::PrivateStorage)?;
            bytes += text.len();
            if bytes > 32_768 { return Err(StoreError::InvalidInput); }
            result.push(SourceProjection { binding: SourceBinding {
                canonical_id: *canonical, version_id: evidence.version_id, version: evidence.version,
                evidence_digest: Sha256::digest(text.as_bytes()).into(),
            }, text });
        }
        Ok(result)
    }.await.map_err(sanitize)
}

/// Exact match only. A source update/tombstone/withdrawal is ineligible: never
/// replace, omit, reorder, or silently use a newer source. Holds version fences
/// through a caller's result commit (migration 0044 guards version insertion).
pub async fn validate_tx(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    approved: &[SourceBinding],
) -> Result<Vec<String>> {
    let ids = approved
        .iter()
        .map(|item| item.canonical_id)
        .collect::<Vec<_>>();
    let projections = prepare_tx(tx, project, &ids).await?;
    if !projections.iter().map(|p| &p.binding).eq(approved) {
        return Err(StoreError::Binding);
    }
    Ok(projections
        .into_iter()
        .map(|projection| projection.text)
        .collect())
}

fn sanitize(error: StoreError) -> StoreError {
    match error {
        StoreError::Database(_) => StoreError::PrivateStorage,
        other => other,
    }
}
