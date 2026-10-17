// SPDX-License-Identifier: AGPL-3.0-or-later
//! Durable local drafts only. No network worker, target-memory writes, consent
//! inference, credential access or reuse of the legacy offline queue.
use chrono::Utc;
use codifix_federation::CanonicalJsonCodec;
use codifix_record::*;
use sqlx::{PgPool, Row, types::Json};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum LaptopContributionError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("invalid local contribution")]
    Invalid,
    #[error("contribution scope binding mismatch")]
    Binding,
    #[error("operation identity already binds different contribution bytes")]
    Conflict,
    #[error("stale contribution transition")]
    StaleTransition,
    #[error("stored contribution is inconsistent")]
    Corrupt,
}
type Result<T> = std::result::Result<T, LaptopContributionError>;

/// The existing pure canonical JSON codec is shared with federation receipts;
/// this digest grants no authority and performs no I/O.
pub fn laptop_contribution_payload_sha256(payload: &LaptopContributionPayload) -> Result<String> {
    payload
        .validate()
        .map_err(|_| LaptopContributionError::Invalid)?;
    digest(payload)
}
fn digest(value: &impl serde::Serialize) -> Result<String> {
    CanonicalJsonCodec::digest(value)
        .map(|d| d.as_hex().to_owned())
        .map_err(|_| LaptopContributionError::Invalid)
}

#[derive(Clone)]
pub struct LaptopContributionStore {
    pool: PgPool,
    binding: LaptopContributionBinding,
}
impl LaptopContributionStore {
    /// Binding is trusted local configuration, not an access grant. No project
    /// projection is read or initialized when opening this store.
    pub fn new(pool: PgPool, binding: LaptopContributionBinding) -> Result<Self> {
        binding
            .validate()
            .map_err(|_| LaptopContributionError::Invalid)?;
        Ok(Self { pool, binding })
    }

    /// One UUID permanently binds full canonical request bytes, including every
    /// source, digest, manifest and user-recorded time. Exact replay is harmless.
    /// The only effect is a draft plus a metadata history entry in local tables.
    pub async fn save_draft(
        &self,
        draft: &CompanyContributionDraft,
    ) -> Result<LaptopContributionMetadata> {
        draft
            .validate()
            .map_err(|_| LaptopContributionError::Invalid)?;
        if draft.binding != self.binding {
            return Err(LaptopContributionError::Binding);
        }
        if laptop_contribution_payload_sha256(&draft.payload)? != draft.payload_sha256 {
            return Err(LaptopContributionError::Invalid);
        }
        let request_sha256 = digest(draft)?;
        let mut tx = self.pool.begin().await?;
        let stored_at = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let metadata = LaptopContributionMetadata {
            schema_version: LAPTOP_CONTRIBUTION_SCHEMA_VERSION,
            operation_id: draft.operation_id,
            binding: draft.binding.clone(),
            request_sha256: request_sha256.clone(),
            payload_sha256: draft.payload_sha256.clone(),
            kind: draft.payload.kind(),
            user_time: draft.user_time.clone(),
            stored_at,
            generation: 0,
            state: LaptopContributionState::Draft,
            blocked_reason: None,
            receipt: None,
        };
        let inserted = sqlx::query("INSERT INTO laptop_contribution_drafts(operation_id,binding,request_sha256,draft,metadata,generation,state,stored_at) VALUES($1,$2,$3,$4,$5,0,'draft',$6) ON CONFLICT(operation_id) DO NOTHING")
            .bind(draft.operation_id).bind(Json(&draft.binding)).bind(&request_sha256)
            .bind(Json(draft)).bind(Json(&metadata)).bind(stored_at).execute(&mut *tx).await?.rows_affected() == 1;
        let row = sqlx::query("SELECT metadata,draft FROM laptop_contribution_drafts WHERE operation_id=$1 FOR UPDATE")
            .bind(draft.operation_id).fetch_one(&mut *tx).await?;
        let prior = row
            .try_get::<Json<LaptopContributionMetadata>, _>("metadata")?
            .0;
        let prior_draft = row.try_get::<Json<CompanyContributionDraft>, _>("draft")?.0;
        if prior.binding != self.binding
            || prior.request_sha256 != request_sha256
            || prior_draft != *draft
        {
            return Err(LaptopContributionError::Conflict);
        }
        self.check_metadata(&prior, draft.operation_id)?;
        if inserted {
            let entry = history_entry(&metadata, stored_at);
            sqlx::query("INSERT INTO laptop_contribution_transitions(operation_id,generation,entry) VALUES($1,0,$2)")
                .bind(draft.operation_id).bind(Json(entry)).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(prior)
    }

    /// Metadata only: body, summary, source references and excerpts are never
    /// fetched by this method. The caller must provide the exact binding.
    pub async fn list_metadata(&self, limit: u32) -> Result<Vec<LaptopContributionMetadata>> {
        if !(1..=100).contains(&limit) {
            return Err(LaptopContributionError::Invalid);
        }
        let rows: Vec<Json<LaptopContributionMetadata>> = sqlx::query_scalar("SELECT metadata FROM laptop_contribution_drafts WHERE binding=$1 ORDER BY stored_at,operation_id LIMIT $2")
            .bind(Json(&self.binding)).bind(i64::from(limit)).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| {
                self.check_metadata(&row.0, row.0.operation_id)?;
                Ok(row.0)
            })
            .collect()
    }

    /// Explicit local payload retrieval. This API has no transport and its
    /// result is not authorization to disclose the payload to an owner/model.
    pub async fn load_draft(&self, operation_id: Uuid) -> Result<CompanyContributionDraft> {
        let row = sqlx::query(
            "SELECT draft,metadata FROM laptop_contribution_drafts WHERE operation_id=$1",
        )
        .bind(operation_id)
        .fetch_one(&self.pool)
        .await?;
        let metadata = row
            .try_get::<Json<LaptopContributionMetadata>, _>("metadata")?
            .0;
        self.check_metadata(&metadata, operation_id)?;
        let draft = row.try_get::<Json<CompanyContributionDraft>, _>("draft")?.0;
        if draft.validate().is_err()
            || draft.binding != self.binding
            || draft.operation_id != operation_id
            || digest(&draft)? != metadata.request_sha256
            || laptop_contribution_payload_sha256(&draft.payload)? != draft.payload_sha256
            || draft.payload_sha256 != metadata.payload_sha256
            || draft.payload.kind() != metadata.kind
            || draft.user_time != metadata.user_time
        {
            return Err(LaptopContributionError::Corrupt);
        }
        Ok(draft)
    }

    /// Records a trusted caller's local observation with generation+state CAS.
    /// It does not send anything or authenticate an owner receipt. A future
    /// edge must recheck current human/device authority and verify transport
    /// before supplying an accepted observation; queued is only local intent.
    pub async fn transition(
        &self,
        operation_id: Uuid,
        change: &LaptopContributionTransition,
    ) -> Result<LaptopContributionMetadata> {
        if change.expected_generation >= i64::MAX as u64
            || change.receipt.is_some() != (change.next_state == LaptopContributionState::Accepted)
            || change.blocked_reason.is_some()
                != (change.next_state == LaptopContributionState::Blocked)
            || !change.expected_state.permits(change.next_state)
        {
            return Err(LaptopContributionError::Invalid);
        }
        let mut tx = self.pool.begin().await?;
        let mut metadata = sqlx::query_scalar::<_, Json<LaptopContributionMetadata>>(
            "SELECT metadata FROM laptop_contribution_drafts WHERE operation_id=$1 FOR UPDATE",
        )
        .bind(operation_id)
        .fetch_one(&mut *tx)
        .await?
        .0;
        self.check_metadata(&metadata, operation_id)?;
        if let Some(receipt) = &change.receipt {
            self.check_receipt(&metadata, receipt)?;
        }
        if metadata.generation == change.expected_generation + 1
            && metadata.state == change.next_state
            && metadata.receipt == change.receipt
            && metadata.blocked_reason == change.blocked_reason
        {
            let prior = sqlx::query_scalar::<_, Json<LaptopContributionHistoryEntry>>("SELECT entry FROM laptop_contribution_transitions WHERE operation_id=$1 AND generation=$2")
                .bind(operation_id).bind(change.expected_generation as i64).fetch_one(&mut *tx).await?.0;
            if prior.state != change.expected_state {
                return Err(LaptopContributionError::StaleTransition);
            }
            tx.commit().await?;
            return Ok(metadata);
        }
        if metadata.generation != change.expected_generation
            || metadata.state != change.expected_state
        {
            return Err(LaptopContributionError::StaleTransition);
        }
        metadata.generation += 1;
        metadata.state = change.next_state;
        metadata.receipt.clone_from(&change.receipt);
        metadata.blocked_reason = change.blocked_reason;
        sqlx::query("UPDATE laptop_contribution_drafts SET metadata=$2,generation=$3,state=$4 WHERE operation_id=$1")
            .bind(operation_id).bind(Json(&metadata)).bind(metadata.generation as i64).bind(metadata.state.as_str())
            .execute(&mut *tx).await?;
        let recorded_at = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO laptop_contribution_transitions(operation_id,generation,entry) VALUES($1,$2,$3)")
            .bind(operation_id).bind(metadata.generation as i64).bind(Json(history_entry(&metadata, recorded_at)))
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(metadata)
    }

    /// Bounded append-only metadata observations, not physical send attempts.
    pub async fn history(
        &self,
        operation_id: Uuid,
        after_generation: Option<u64>,
        limit: u32,
    ) -> Result<Vec<LaptopContributionHistoryEntry>> {
        if !(1..=100).contains(&limit) || after_generation.is_some_and(|n| n > i64::MAX as u64) {
            return Err(LaptopContributionError::Invalid);
        }
        let mut tx = self.pool.begin().await?;
        let metadata = sqlx::query_scalar::<_, Json<LaptopContributionMetadata>>(
            "SELECT metadata FROM laptop_contribution_drafts WHERE operation_id=$1 FOR SHARE",
        )
        .bind(operation_id)
        .fetch_one(&mut *tx)
        .await?
        .0;
        self.check_metadata(&metadata, operation_id)?;
        let entries: Vec<Json<LaptopContributionHistoryEntry>> = sqlx::query_scalar("SELECT entry FROM laptop_contribution_transitions WHERE operation_id=$1 AND generation>$2 ORDER BY generation LIMIT $3")
            .bind(operation_id).bind(after_generation.map(|n| n as i64).unwrap_or(-1))
            .bind(i64::from(limit)).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(entries.into_iter().map(|e| e.0).collect())
    }

    fn check_metadata(
        &self,
        metadata: &LaptopContributionMetadata,
        operation_id: Uuid,
    ) -> Result<()> {
        if metadata.binding != self.binding {
            return Err(LaptopContributionError::Binding);
        }
        if metadata.schema_version != LAPTOP_CONTRIBUTION_SCHEMA_VERSION
            || metadata.operation_id != operation_id
            || operation_id.is_nil()
            || metadata.generation > i64::MAX as u64
            || !contribution_sha256_valid(&metadata.request_sha256)
            || !contribution_sha256_valid(&metadata.payload_sha256)
            || metadata.receipt.is_some() != (metadata.state == LaptopContributionState::Accepted)
            || metadata.blocked_reason.is_some()
                != (metadata.state == LaptopContributionState::Blocked)
        {
            return Err(LaptopContributionError::Corrupt);
        }
        if let Some(receipt) = &metadata.receipt {
            self.check_receipt(metadata, receipt)?;
        }
        Ok(())
    }
    fn check_receipt(
        &self,
        metadata: &LaptopContributionMetadata,
        receipt: &LaptopContributionReceipt,
    ) -> Result<()> {
        if receipt.schema_version != LAPTOP_CONTRIBUTION_SCHEMA_VERSION
            || receipt.operation_id != metadata.operation_id
            || receipt.binding != metadata.binding
            || receipt.request_sha256 != metadata.request_sha256
            || receipt.payload_sha256 != metadata.payload_sha256
            || receipt.kind != metadata.kind
            || receipt.owner_record_id.is_nil()
        {
            return Err(LaptopContributionError::Binding);
        }
        Ok(())
    }
}
fn history_entry(
    metadata: &LaptopContributionMetadata,
    recorded_at: chrono::DateTime<Utc>,
) -> LaptopContributionHistoryEntry {
    LaptopContributionHistoryEntry {
        generation: metadata.generation,
        state: metadata.state,
        blocked_reason: metadata.blocked_reason,
        receipt: metadata.receipt.clone(),
        recorded_at,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn fixture_draft() -> CompanyContributionDraft {
        let payload = LaptopContributionPayload::RawCapture {
            text: "offline fixture body SECRET-MARKER".into(),
            sources: vec![LaptopContributionSource {
                kind: LaptopContributionSourceKind::File,
                reference: "private-local-path".into(),
                excerpt: Some("PRIVATE-EXCERPT".into()),
            }],
        };
        CompanyContributionDraft {
            schema_version: 1,
            operation_id: Uuid::new_v4(),
            binding: LaptopContributionBinding {
                scope: LaptopContributionScope {
                    company_id: Uuid::new_v4(),
                    area: OnboardingArea::Workshop,
                    owner_instance_id: Uuid::new_v4(),
                    project_id: Uuid::new_v4(),
                    space_id: Uuid::new_v4(),
                },
                device_id: Uuid::new_v4(),
                manifest_revision: 2,
                manifest_sha256: "a".repeat(64),
            },
            payload_sha256: laptop_contribution_payload_sha256(&payload).unwrap(),
            user_time: LaptopContributionTime {
                recorded_at: Utc::now(),
                basis: LaptopContributionTimeBasis::UserRecordedDeviceTime,
            },
            payload,
        }
    }
    fn change(
        metadata: &LaptopContributionMetadata,
        next: LaptopContributionState,
    ) -> LaptopContributionTransition {
        LaptopContributionTransition {
            expected_generation: metadata.generation,
            expected_state: metadata.state,
            next_state: next,
            blocked_reason: None,
            receipt: None,
        }
    }
    #[tokio::test]
    async fn exact_replay_scope_conflicts_sanitized_lists_and_no_projection_effects() {
        let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let pool = db.pool.clone();
        let draft = fixture_draft();
        let store = LaptopContributionStore::new(pool.clone(), draft.binding.clone()).unwrap();
        let subscription = Uuid::new_v4();
        let delivered_scope = serde_json::json!({"company": draft.binding.scope.company_id,
            "area": "workshop", "owner": draft.binding.scope.owner_instance_id,
            "project": draft.binding.scope.project_id, "space": draft.binding.scope.space_id});
        let delivered_projection = serde_json::json!({"records": [], "reviews": []});
        sqlx::query("INSERT INTO onboarding_project_projections(subscription,device,scope,cursor,projection) VALUES($1,$2,$3,7,$4)")
            .bind(subscription).bind(draft.binding.device_id).bind(Json(&delivered_scope))
            .bind(Json(&delivered_projection)).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO federation_replicas(space,receiver,authority,cursor,projection) VALUES($1,$2,$3,11,$4)")
            .bind(draft.binding.scope.space_id).bind(draft.binding.device_id).bind(draft.binding.scope.owner_instance_id)
            .bind(Json(&delivered_projection)).execute(&pool).await.unwrap();
        let before: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM memory_entries),(SELECT count(*) FROM onboarding_project_projections),(SELECT count(*) FROM onboarding_project_pages)")
            .fetch_one(&pool).await.unwrap();
        let saved = store.save_draft(&draft).await.unwrap();
        assert_eq!(saved, store.save_draft(&draft).await.unwrap());
        assert_eq!(draft, store.load_draft(draft.operation_id).await.unwrap());
        let serialized = serde_json::to_string(&store.list_metadata(10).await.unwrap()).unwrap();
        for forbidden in [
            "SECRET-MARKER",
            "private-local-path",
            "PRIVATE-EXCERPT",
            "\"payload\"",
        ] {
            assert!(!serialized.contains(forbidden));
        }
        let mut altered = draft.clone();
        altered.user_time.recorded_at += chrono::Duration::seconds(1);
        assert!(matches!(
            store.save_draft(&altered).await,
            Err(LaptopContributionError::Conflict)
        ));
        altered = draft.clone();
        if let LaptopContributionPayload::RawCapture { sources, .. } = &mut altered.payload {
            sources[0].excerpt = Some("altered source".into());
        }
        assert!(matches!(
            store.save_draft(&altered).await,
            Err(LaptopContributionError::Invalid)
        ));
        altered.payload_sha256 = laptop_contribution_payload_sha256(&altered.payload).unwrap();
        assert!(matches!(
            store.save_draft(&altered).await,
            Err(LaptopContributionError::Conflict)
        ));
        let mut other_binding = draft.binding.clone();
        other_binding.scope.project_id = Uuid::new_v4();
        let other = LaptopContributionStore::new(pool.clone(), other_binding.clone()).unwrap();
        altered = draft.clone();
        altered.binding = other_binding;
        assert!(matches!(
            other.save_draft(&altered).await,
            Err(LaptopContributionError::Conflict)
        ));
        assert!(other.list_metadata(10).await.unwrap().is_empty());
        assert!(matches!(
            other.load_draft(draft.operation_id).await,
            Err(LaptopContributionError::Binding)
        ));
        let after: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM memory_entries),(SELECT count(*) FROM onboarding_project_projections),(SELECT count(*) FROM onboarding_project_pages)")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(before, after);
        let projection: (i64, Json<serde_json::Value>, Json<serde_json::Value>) = sqlx::query_as(
            "SELECT cursor,scope,projection FROM onboarding_project_projections WHERE subscription=$1")
            .bind(subscription).fetch_one(&pool).await.unwrap();
        assert_eq!(
            projection,
            (7, Json(delivered_scope), Json(delivered_projection.clone()))
        );
        let replica: (i64, Json<serde_json::Value>) = sqlx::query_as(
            "SELECT cursor,projection FROM federation_replicas WHERE space=$1 AND receiver=$2",
        )
        .bind(draft.binding.scope.space_id)
        .bind(draft.binding.device_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(replica, (11, Json(delivered_projection)));
        assert_eq!(
            store
                .history(draft.operation_id, None, 10)
                .await
                .unwrap()
                .len(),
            1
        );
        db.cleanup().await.unwrap();
    }
    #[tokio::test]
    async fn generation_cas_receipt_binding_completed_immutability_and_atomic_history() {
        let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let pool = db.pool.clone();
        let draft = fixture_draft();
        let store = LaptopContributionStore::new(pool.clone(), draft.binding.clone()).unwrap();
        let initial = store.save_draft(&draft).await.unwrap();
        assert!(sqlx::query("UPDATE laptop_contribution_drafts SET draft=jsonb_set(draft,'{payload,text}','\"altered\"'::jsonb) WHERE operation_id=$1")
            .bind(draft.operation_id).execute(&pool).await.is_err());
        assert!(
            sqlx::query("DELETE FROM laptop_contribution_drafts WHERE operation_id=$1")
                .bind(draft.operation_id)
                .execute(&pool)
                .await
                .is_err()
        );
        assert!(
            sqlx::query("DELETE FROM laptop_contribution_transitions WHERE operation_id=$1")
                .bind(draft.operation_id)
                .execute(&pool)
                .await
                .is_err()
        );
        let queued_change = change(&initial, LaptopContributionState::Queued);
        let queued = store
            .transition(draft.operation_id, &queued_change)
            .await
            .unwrap();
        assert_eq!(
            queued,
            store
                .transition(draft.operation_id, &queued_change)
                .await
                .unwrap()
        );
        let uncertain = store
            .transition(
                draft.operation_id,
                &change(&queued, LaptopContributionState::Uncertain),
            )
            .await
            .unwrap();
        assert!(matches!(
            store.transition(draft.operation_id, &queued_change).await,
            Err(LaptopContributionError::StaleTransition)
        ));
        let receipt = LaptopContributionReceipt {
            schema_version: 1,
            operation_id: draft.operation_id,
            binding: draft.binding.clone(),
            request_sha256: initial.request_sha256.clone(),
            payload_sha256: draft.payload_sha256.clone(),
            kind: draft.payload.kind(),
            owner_record_id: Uuid::new_v4(),
            owner_committed_at: Utc::now(),
        };
        let mut accepted_change = change(&uncertain, LaptopContributionState::Accepted);
        let mut wrong = receipt.clone();
        wrong.payload_sha256 = "b".repeat(64);
        accepted_change.receipt = Some(wrong);
        assert!(matches!(
            store.transition(draft.operation_id, &accepted_change).await,
            Err(LaptopContributionError::Binding)
        ));
        accepted_change.receipt = Some(receipt);
        let accepted = store
            .transition(draft.operation_id, &accepted_change)
            .await
            .unwrap();
        assert_eq!(
            accepted,
            store
                .transition(draft.operation_id, &accepted_change)
                .await
                .unwrap()
        );
        assert_eq!(accepted, store.save_draft(&draft).await.unwrap());
        assert!(
            sqlx::query(
                "UPDATE laptop_contribution_drafts SET metadata=metadata WHERE operation_id=$1"
            )
            .bind(draft.operation_id)
            .execute(&pool)
            .await
            .is_err()
        );
        assert!(
            store
                .transition(
                    draft.operation_id,
                    &change(&accepted, LaptopContributionState::Queued)
                )
                .await
                .is_err()
        );
        assert_eq!(
            store
                .history(draft.operation_id, None, 10)
                .await
                .unwrap()
                .len(),
            4
        );
        let another = fixture_draft();
        let another_store =
            LaptopContributionStore::new(pool.clone(), another.binding.clone()).unwrap();
        let another_saved = another_store.save_draft(&another).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION fail_local_history() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic local journal fault'; END $$; CREATE TRIGGER fail_local_history BEFORE INSERT ON laptop_contribution_transitions FOR EACH ROW EXECUTE FUNCTION fail_local_history();")
            .execute(&pool).await.unwrap();
        assert!(
            another_store
                .transition(
                    another.operation_id,
                    &change(&another_saved, LaptopContributionState::Queued)
                )
                .await
                .is_err()
        );
        assert_eq!(
            another_saved,
            another_store.list_metadata(10).await.unwrap()[0]
        );
        assert_eq!(
            another_store
                .history(another.operation_id, None, 10)
                .await
                .unwrap()
                .len(),
            1
        );
        db.cleanup().await.unwrap();
    }
    #[tokio::test]
    async fn concurrent_local_transitions_have_one_cas_winner() {
        let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let draft = fixture_draft();
        let store = LaptopContributionStore::new(db.pool.clone(), draft.binding.clone()).unwrap();
        let initial = store.save_draft(&draft).await.unwrap();
        let queued = change(&initial, LaptopContributionState::Queued);
        let mut blocked = change(&initial, LaptopContributionState::Blocked);
        blocked.blocked_reason = Some(LaptopContributionBlockReason::ConsentRequired);
        let (a, b) = tokio::join!(
            store.transition(draft.operation_id, &queued),
            store.transition(draft.operation_id, &blocked)
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        assert!(
            matches!(a, Err(LaptopContributionError::StaleTransition))
                || matches!(b, Err(LaptopContributionError::StaleTransition))
        );
        assert_eq!(
            store
                .history(draft.operation_id, None, 10)
                .await
                .unwrap()
                .len(),
            2
        );
        db.cleanup().await.unwrap();
    }
    #[test]
    fn digest_binds_every_source_and_add_proposals_are_pending_only() {
        let original = fixture_draft();
        let proposal = LaptopContributionPayload::PendingAddProposal {
            canonical_text: "proposed offline knowledge".into(),
            summary: "pending human review".into(),
            memory_type: MemoryType::Implementation,
            sources: vec![],
        };
        assert!(laptop_contribution_payload_sha256(&proposal).is_ok());
        assert_ne!(
            laptop_contribution_payload_sha256(&proposal).unwrap(),
            original.payload_sha256
        );
        let mut binding = original.binding;
        binding.scope.area = OnboardingArea::Trust;
        assert!(binding.validate().is_err());
    }
}
