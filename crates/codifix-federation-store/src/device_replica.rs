// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private curated project feeds. Callers hold current device/human authority;
//! this trusted store supplies no permission and never writes shared publications.
use crate::{Result, StoreError, audit_digest};
use codifix_federation::{device_replica::*, subscription::ProjectScope};
use sqlx::{Postgres, Row, Transaction, types::Json};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceMemoryHit {
    pub scope: ProjectScope,
    pub version: DeviceMemoryVersion,
    pub state: DeviceCanonicalState,
    pub digest: codifix_federation::ContentDigest,
    pub read_only: bool,
}
#[derive(Clone)]
pub struct DeviceProjectionStore {
    pool: sqlx::PgPool,
    subscription: Uuid,
    device: Uuid,
    scope: ProjectScope,
}
impl DeviceProjectionStore {
    pub async fn initialize(
        pool: sqlx::PgPool,
        subscription: Uuid,
        device: Uuid,
        scope: ProjectScope,
    ) -> Result<Self> {
        let mut tx = pool.begin().await?;
        initialize_device_projection(&mut tx, subscription, device, &scope).await?;
        tx.commit().await?;
        Ok(Self {
            pool,
            subscription,
            device,
            scope,
        })
    }
    pub async fn open(
        pool: sqlx::PgPool,
        subscription: Uuid,
        device: Uuid,
        scope: ProjectScope,
    ) -> Result<Self> {
        let result = Self {
            pool,
            subscription,
            device,
            scope,
        };
        result.status().await?;
        Ok(result)
    }
    async fn inspect(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<crate::ProjectProjectionStatus> {
        let row=sqlx::query("SELECT device,scope,cursor,observed_at FROM device_replica_projections WHERE subscription=$1 FOR SHARE")
            .bind(self.subscription).fetch_one(&mut **tx).await?;
        if row.try_get::<Uuid, _>("device")? != self.device
            || row.try_get::<Json<ProjectScope>, _>("scope")?.0 != self.scope
        {
            return Err(StoreError::Binding);
        }
        Ok(crate::ProjectProjectionStatus {
            cursor: row.try_get::<i64, _>("cursor")? as u64,
            observed_at: row.try_get("observed_at")?,
        })
    }
    pub async fn status(&self) -> Result<crate::ProjectProjectionStatus> {
        let mut tx = self.pool.begin().await?;
        let status = self.inspect(&mut tx).await?;
        tx.commit().await?;
        Ok(status)
    }
    pub async fn apply_verified(&self, page: &DeviceReplicaPage) -> Result<u64> {
        let mut tx = self.pool.begin().await?;
        let cursor = apply_device_replica_page(&mut tx, self.subscription, page).await?;
        tx.commit().await?;
        Ok(cursor)
    }
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<DeviceMemoryHit>> {
        Ok(self.search_with_status(query, limit).await?.1)
    }
    /// Current owner rows for explicit local browsing, including archived rows.
    /// This does not alter search, publication, or the local mutation tables.
    pub async fn browse(&self) -> Result<Vec<DeviceMemoryHit>> {
        let mut tx = self.pool.begin().await?;
        self.inspect(&mut tx).await?;
        let rows: Vec<(Json<DeviceReplicaRecord>, Json<DeviceReplicaRecord>)> = sqlx::query_as(
            "WITH latest AS (
               SELECT DISTINCT ON (record->'value'->>'canonical_id') record
               FROM device_replica_records WHERE subscription=$1 AND record->>'kind'='memory_version'
               ORDER BY record->'value'->>'canonical_id',(record->'value'->>'version')::bigint DESC
             ) SELECT v.record,s.record FROM device_replica_records s
             JOIN latest v ON v.record->'value'->>'id'=s.record->'value'->>'current_version_id'
               AND v.record->'value'->>'canonical_id'=s.record->'value'->>'canonical_id'
             WHERE s.subscription=$1 AND s.record->>'kind'='canonical_state'
             ORDER BY s.record_key LIMIT 100001",
        ).bind(self.subscription).fetch_all(&mut *tx).await?;
        if rows.len() > 100_000 {
            return Err(StoreError::Limits);
        }
        tx.commit().await?;
        let mut hits = Vec::new();
        for (Json(record), Json(state)) in rows {
            let digest = audit_digest(&record)?;
            if let (
                DeviceReplicaRecord::MemoryVersion(version),
                DeviceReplicaRecord::CanonicalState(state),
            ) = (record, state)
                && !version.tombstone
            {
                hits.push(DeviceMemoryHit {
                    scope: self.scope.clone(),
                    version,
                    state,
                    digest,
                    read_only: true,
                });
            }
        }
        Ok(hits)
    }
    pub async fn search_with_status(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<(crate::ProjectProjectionStatus, Vec<DeviceMemoryHit>)> {
        if query.trim().is_empty() || query.len() > 4096 || !(1..=50).contains(&limit) {
            return Err(StoreError::Limits);
        }
        let mut tx = self.pool.begin().await?;
        let status = self.inspect(&mut tx).await?;
        let records: Vec<Json<DeviceReplicaRecord>> = sqlx::query_scalar(
            "SELECT record FROM device_replica_records WHERE subscription=$1 ORDER BY record_key",
        )
        .bind(self.subscription)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let mut versions = BTreeMap::<Uuid, DeviceMemoryVersion>::new();
        let mut states = BTreeMap::new();
        for Json(record) in records {
            match record {
                DeviceReplicaRecord::MemoryVersion(v) => {
                    if versions
                        .get(&v.canonical_id)
                        .is_none_or(|p| p.version < v.version)
                    {
                        versions.insert(v.canonical_id, v);
                    }
                }
                DeviceReplicaRecord::CanonicalState(s) => {
                    states.insert(s.canonical_id, s);
                }
                _ => {}
            }
        }
        let terms = codifix_record::text::extract_lexical_terms(query);
        let mut hits = Vec::new();
        for (canonical, version) in versions {
            let Some(state) = states.remove(&canonical) else {
                continue;
            };
            if version.tombstone
                || state.status != codifix_record::MemoryStatus::Active
                || state.current_version_id != version.id
            {
                continue;
            }
            let score = codifix_record::text::lexical_overlap_ratio(
                &format!("{} {}", version.text, version.summary).to_lowercase(),
                &terms,
            );
            if score == 0.0 {
                continue;
            }
            let digest = audit_digest(&DeviceReplicaRecord::MemoryVersion(version.clone()))?;
            hits.push((
                score,
                DeviceMemoryHit {
                    scope: self.scope.clone(),
                    version,
                    state,
                    digest,
                    read_only: true,
                },
            ));
        }
        hits.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| a.1.version.id.cmp(&b.1.version.id))
        });
        Ok((
            status,
            hits.into_iter()
                .take(limit as usize)
                .map(|(_, hit)| hit)
                .collect(),
        ))
    }
    pub async fn history(&self, canonical: Uuid) -> Result<Vec<DeviceMemoryVersion>> {
        let mut tx = self.pool.begin().await?;
        self.inspect(&mut tx).await?;
        let records:Vec<Json<DeviceReplicaRecord>>=sqlx::query_scalar("SELECT record FROM device_replica_records WHERE subscription=$1 AND record->>'kind'='memory_version' AND record->'value'->>'canonical_id'=$2 ORDER BY (record->'value'->>'version')::integer")
            .bind(self.subscription).bind(canonical.to_string()).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(records
            .into_iter()
            .filter_map(|Json(v)| match v {
                DeviceReplicaRecord::MemoryVersion(v) => Some(v),
                _ => None,
            })
            .collect())
    }
}

impl crate::GatewayStore {
    /// The origin asserts its own device page. No payload is persisted in the
    /// gateway, published to the PDS, or made available through space pull.
    pub async fn authorize_device_replica_attestation(
        &self,
        fence: crate::WriterFence,
        actor: codifix_federation::publication::InstanceId,
        page: &DeviceReplicaPage,
    ) -> Result<codifix_federation::publication::InstanceId> {
        if !page.valid()
            || page.request.scope.owner != actor
            || page.request.scope.space != fence.space
        {
            return Err(StoreError::Binding);
        }
        let mut tx = self.pool.begin().await?;
        let authority = self.project_actor(&mut tx, fence, actor).await?;
        tx.commit().await?;
        Ok(authority)
    }
}

/// A single SQL statement takes one MVCC snapshot across all selected tables.
/// Only the listed fields can enter the wire record; no SELECT row_to_json(*).
pub async fn curated_project_records(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
) -> Result<Vec<DeviceReplicaRecord>> {
    if project.is_nil() {
        return Err(StoreError::Binding);
    }
    let rows: Vec<Json<DeviceReplicaRecord>> = sqlx::query_scalar(r#"
WITH versions AS MATERIALIZED (SELECT * FROM memory_entries WHERE project_id=$1),
records AS (
 SELECT jsonb_build_object('kind','memory_version','value',jsonb_build_object(
 'id',m.id,'canonical_id',m.canonical_id,'version',m.version_no,'tombstone',m.is_tombstone,
 'text',m.canonical_text,'summary',m.summary,'memory_type',m.memory_type,'scope',m.scope,
 'author',COALESCE((SELECT NULLIF(s.writer_id,'') FROM memory_sources ms JOIN tasks t ON t.id=ms.task_id JOIN sessions s ON s.id=t.session_id WHERE ms.memory_entry_id=m.id AND s.project_id=$1 ORDER BY ms.created_at,ms.id LIMIT 1),(SELECT c.human_principal_id::text FROM device_curated_contribution_receipts c LEFT JOIN device_curated_contribution_resolutions r USING(operation_id) WHERE COALESCE(r.resolution_json->>'owner_memory_id',c.receipt_json->>'owner_memory_id')=m.id::text LIMIT 1),'unknown:local-author'),
 'authored_at',m.created_at,'updated_at',m.updated_at,
 'tags',COALESCE((SELECT jsonb_agg(tag ORDER BY tag) FROM memory_tags WHERE memory_entry_id=m.id),'[]'::jsonb),
 'unavailable_source_ids',COALESCE((SELECT jsonb_agg(id ORDER BY id) FROM memory_sources WHERE memory_entry_id=m.id),'[]'::jsonb))) AS record FROM versions m
 UNION ALL
 SELECT jsonb_build_object('kind','canonical_state','value',jsonb_build_object(
 'canonical_id',s.canonical_id,'current_version_id',(SELECT v.id FROM versions v WHERE v.canonical_id=s.canonical_id ORDER BY v.version_no DESC LIMIT 1),'status',s.status,'confidence',round(s.confidence::numeric*1000000)::integer,
 'importance',s.importance,'updated_at',s.updated_at,'archived_at',s.archived_at))
 FROM memory_canonical_state s WHERE s.project_id=$1 AND EXISTS(SELECT 1 FROM versions v WHERE v.canonical_id=s.canonical_id)
 UNION ALL
 SELECT jsonb_build_object('kind','relation','value',jsonb_build_object(
 'id',r.id,'source_canonical',r.src_canonical_id,'target_canonical',r.dst_canonical_id,'relation_type',r.relation_type))
 FROM memory_relations r WHERE r.origin='asserted'
 AND EXISTS(SELECT 1 FROM versions a WHERE a.canonical_id=r.src_canonical_id)
 AND EXISTS(SELECT 1 FROM versions b WHERE b.canonical_id=r.dst_canonical_id)
)
SELECT record FROM records LIMIT 100001
"#).bind(project).fetch_all(&mut **tx).await?;
    if rows.len() > 100_000 {
        return Err(StoreError::Limits);
    }
    let mut records = Vec::with_capacity(rows.len());
    for Json(record) in rows {
        if !record.valid() {
            return Err(StoreError::Binding);
        }
        records.push(record);
    }
    records.sort_by_key(DeviceReplicaRecord::key);
    Ok(records)
}

/// Hold the per-scope counter through commit. Concurrent refreshes cannot expose
/// a cursor past an uncommitted event. Source rows are selected after this lock.
pub async fn refresh_device_replica(
    tx: &mut Transaction<'_, Postgres>,
    scope: &ProjectScope,
) -> Result<u64> {
    if !scope.valid() {
        return Err(StoreError::Binding);
    }
    sqlx::query("INSERT INTO device_replica_heads(scope) VALUES($1) ON CONFLICT DO NOTHING")
        .bind(Json(scope))
        .execute(&mut **tx)
        .await?;
    let mut seq: i64 =
        sqlx::query_scalar("SELECT sequence FROM device_replica_heads WHERE scope=$1 FOR UPDATE")
            .bind(Json(scope))
            .fetch_one(&mut **tx)
            .await?;
    let current = curated_project_records(tx, scope.project)
        .await?
        .into_iter()
        .map(|v| (v.key(), v))
        .collect::<BTreeMap<_, _>>();
    let rows = sqlx::query("SELECT record_key,record FROM device_replica_sources WHERE scope=$1")
        .bind(Json(scope))
        .fetch_all(&mut **tx)
        .await?;
    let mut previous = BTreeMap::new();
    for row in rows {
        previous.insert(
            row.try_get::<String, _>("record_key")?,
            row.try_get::<Json<DeviceReplicaRecord>, _>("record")?.0,
        );
    }
    let mut changes = Vec::new();
    for (key, record) in &current {
        if previous.get(key) != Some(record) {
            changes.push((key.clone(), Some(record.clone())));
        }
    }
    for key in previous.keys() {
        if !current.contains_key(key) {
            changes.push((key.clone(), None));
        }
    }
    for (key, record) in changes {
        seq = seq.checked_add(1).ok_or(StoreError::Limits)?;
        let entry = DeviceReplicaEntry {
            sequence: seq as u64,
            key: key.clone(),
            digest: audit_digest(&record)?,
            record: record.clone(),
        };
        sqlx::query("INSERT INTO device_replica_events(scope,sequence,entry) VALUES($1,$2,$3)")
            .bind(Json(scope))
            .bind(seq)
            .bind(Json(entry))
            .execute(&mut **tx)
            .await?;
        if let Some(record) = record {
            sqlx::query("INSERT INTO device_replica_sources(scope,record_key,record) VALUES($1,$2,$3) ON CONFLICT(scope,record_key) DO UPDATE SET record=EXCLUDED.record")
                .bind(Json(scope)).bind(key).bind(Json(record)).execute(&mut **tx).await?;
        } else {
            sqlx::query("DELETE FROM device_replica_sources WHERE scope=$1 AND record_key=$2")
                .bind(Json(scope))
                .bind(key)
                .execute(&mut **tx)
                .await?;
        }
    }
    sqlx::query("UPDATE device_replica_heads SET sequence=$2 WHERE scope=$1")
        .bind(Json(scope))
        .bind(seq)
        .execute(&mut **tx)
        .await?;
    Ok(seq as u64)
}

pub async fn device_replica_page(
    tx: &mut Transaction<'_, Postgres>,
    request: &DeviceReplicaRequest,
) -> Result<DeviceReplicaPage> {
    if !request.valid() {
        return Err(StoreError::Binding);
    }
    let head: Option<i64> =
        sqlx::query_scalar("SELECT sequence FROM device_replica_heads WHERE scope=$1")
            .bind(Json(&request.scope))
            .fetch_optional(&mut **tx)
            .await?;
    let head = head.ok_or(StoreError::Resync)? as u64;
    let through = request.through.unwrap_or(head);
    if through > head || request.after > through {
        return Err(StoreError::Resync);
    }
    let rows:Vec<Json<DeviceReplicaEntry>> = sqlx::query_scalar("SELECT entry FROM device_replica_events WHERE scope=$1 AND sequence>$2 AND sequence<=$3 ORDER BY sequence LIMIT $4")
        .bind(Json(&request.scope)).bind(request.after as i64).bind(through as i64)
        .bind(i64::from(request.limit)).fetch_all(&mut **tx).await?;
    let page = DeviceReplicaPage {
        request: request.clone(),
        through,
        next: request.after + rows.len() as u64,
        entries: rows.into_iter().map(|r| r.0).collect(),
    };
    if !page.valid() {
        return Err(StoreError::Corrupt);
    }
    Ok(page)
}

pub async fn initialize_device_projection(
    tx: &mut Transaction<'_, Postgres>,
    subscription: Uuid,
    device: Uuid,
    scope: &ProjectScope,
) -> Result<()> {
    if subscription.is_nil() || device.is_nil() || !scope.valid() {
        return Err(StoreError::Binding);
    }
    sqlx::query("INSERT INTO device_replica_projections(subscription,device,scope) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
        .bind(subscription).bind(device).bind(Json(scope)).execute(&mut **tx).await?;
    let row = sqlx::query(
        "SELECT device,scope FROM device_replica_projections WHERE subscription=$1 FOR SHARE",
    )
    .bind(subscription)
    .fetch_one(&mut **tx)
    .await?;
    if row.try_get::<Uuid, _>("device")? != device
        || row.try_get::<Json<ProjectScope>, _>("scope")?.0 != *scope
    {
        return Err(StoreError::Binding);
    }
    Ok(())
}

/// Transport verification must precede this operation. Database projection and
/// cursor commit together; a replay receipt never grants new delivery authority.
pub async fn apply_device_replica_page(
    tx: &mut Transaction<'_, Postgres>,
    subscription: Uuid,
    page: &DeviceReplicaPage,
) -> Result<u64> {
    if !page.valid() {
        return Err(StoreError::Binding);
    }
    let row=sqlx::query("SELECT device,scope,cursor FROM device_replica_projections WHERE subscription=$1 FOR UPDATE")
        .bind(subscription).fetch_one(&mut **tx).await?;
    if row.try_get::<Uuid, _>("device")? != page.request.device
        || row.try_get::<Json<ProjectScope>, _>("scope")?.0 != page.request.scope
    {
        return Err(StoreError::Binding);
    }
    let digest = audit_digest(page)?.as_hex().to_owned();
    if let Some(replay) = sqlx::query(
        "SELECT digest,next_cursor FROM device_replica_pages WHERE subscription=$1 AND nonce=$2",
    )
    .bind(subscription)
    .bind(page.request.nonce)
    .fetch_optional(&mut **tx)
    .await?
    {
        if replay.try_get::<String, _>("digest")? != digest {
            return Err(StoreError::Binding);
        }
        return Ok(replay.try_get::<i64, _>("next_cursor")? as u64);
    }
    if row.try_get::<i64, _>("cursor")? as u64 != page.request.after {
        return Err(StoreError::Resync);
    }
    for entry in &page.entries {
        if let Some(record) = &entry.record {
            match record {
                DeviceReplicaRecord::MemoryVersion(v) => {
                    sqlx::query("INSERT INTO device_replica_owner_watermarks(subscription,canonical_id,version) VALUES($1,$2,$3) ON CONFLICT(subscription,canonical_id) DO UPDATE SET version=GREATEST(device_replica_owner_watermarks.version,EXCLUDED.version)")
                        .bind(subscription).bind(v.canonical_id).bind(i64::from(v.version)).execute(&mut **tx).await?;
                }
                DeviceReplicaRecord::CanonicalState(s) => {
                    sqlx::query("INSERT INTO device_replica_owner_watermarks(subscription,canonical_id,inactive) VALUES($1,$2,$3) ON CONFLICT(subscription,canonical_id) DO UPDATE SET inactive=EXCLUDED.inactive")
                        .bind(subscription).bind(s.canonical_id).bind(s.status != codifix_record::MemoryStatus::Active).execute(&mut **tx).await?;
                }
                DeviceReplicaRecord::Relation(_) => {}
            }
            sqlx::query("INSERT INTO device_replica_records(subscription,record_key,record) VALUES($1,$2,$3) ON CONFLICT(subscription,record_key) DO UPDATE SET record=EXCLUDED.record")
                .bind(subscription).bind(&entry.key).bind(Json(record)).execute(&mut **tx).await?;
        } else {
            sqlx::query(
                "DELETE FROM device_replica_records WHERE subscription=$1 AND record_key=$2",
            )
            .bind(subscription)
            .bind(&entry.key)
            .execute(&mut **tx)
            .await?;
        }
    }
    sqlx::query("INSERT INTO device_replica_pages(subscription,nonce,digest,next_cursor) VALUES($1,$2,$3,$4)")
        .bind(subscription).bind(page.request.nonce).bind(digest).bind(page.next as i64).execute(&mut **tx).await?;
    sqlx::query("UPDATE device_replica_projections SET cursor=$2,observed_at=clock_timestamp() WHERE subscription=$1")
        .bind(subscription).bind(page.next as i64).execute(&mut **tx).await?;
    Ok(page.next)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use codifix_federation::publication::{InstanceId, SpaceId};

    #[tokio::test]
    async fn device_replica_only_syncs_asserted_same_project_relations() {
        let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let project = Uuid::new_v4();
        let other = Uuid::new_v4();
        for id in [project, other] {
            sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'fixture','/private/repo')")
                .bind(id).bind(id.to_string()).execute(&db.pool).await.unwrap();
        }
        // 318 * 317 = 100,806 distinct heuristic edges: the old selection
        // exhausted the 100,000-record bound before producing a usable feed.
        let memories: Vec<Uuid> = sqlx::query_scalar("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) SELECT id,$1,id,1,false,'curated fixture','summary','project','project',3,0.8,now(),now(),to_tsvector('simple','curated fixture') FROM (SELECT md5($1::text || ':' || n::text)::uuid AS id FROM generate_series(1,318) n) fixture RETURNING id")
            .bind(project).fetch_all(&db.pool).await.unwrap();
        let foreign = Uuid::new_v4();
        sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,false,'foreign','summary','project','project',3,0.8,now(),now(),to_tsvector('simple','foreign'))")
            .bind(foreign).bind(other).execute(&db.pool).await.unwrap();
        let derived = sqlx::query("INSERT INTO memory_relations(id,src_canonical_id,dst_canonical_id,relation_type,origin) SELECT md5(a.id::text || b.id::text)::uuid,a.id,b.id,'supports','derived' FROM memory_entries a CROSS JOIN memory_entries b WHERE a.project_id=$1 AND b.project_id=$1 AND a.id<>b.id")
            .bind(project).execute(&db.pool).await.unwrap();
        assert!(derived.rows_affected() > 100_000);
        let reclassified = Uuid::new_v4();
        let removed = Uuid::new_v4();
        for (id, source, target) in [
            (reclassified, memories[0], memories[1]),
            (removed, memories[1], memories[0]),
            (Uuid::new_v4(), memories[0], foreign),
            (Uuid::new_v4(), foreign, memories[0]),
        ] {
            sqlx::query("INSERT INTO memory_relations(id,src_canonical_id,dst_canonical_id,relation_type,origin) VALUES($1,$2,$3,'summarizes','asserted')")
                .bind(id).bind(source).bind(target).execute(&db.pool).await.unwrap();
        }
        let scope = ProjectScope {
            company: Uuid::new_v4(),
            area: "workshop".into(),
            owner: InstanceId::new(Uuid::new_v4()).unwrap(),
            project,
            space: SpaceId::new(Uuid::new_v4()).unwrap(),
        };
        let mut tx = db.pool.begin().await.unwrap();
        let records = curated_project_records(&mut tx, project).await.unwrap();
        assert_eq!(records.len(), 318 * 2 + 2);
        let mut relation_ids = records
            .iter()
            .filter_map(|record| match record {
                DeviceReplicaRecord::Relation(relation) => Some(relation.id),
                _ => None,
            })
            .collect::<Vec<_>>();
        relation_ids.sort();
        let mut expected = vec![reclassified, removed];
        expected.sort();
        assert_eq!(relation_ids, expected);
        let head = refresh_device_replica(&mut tx, &scope).await.unwrap();
        assert_eq!(head, records.len() as u64);
        let subscription = Uuid::new_v4();
        let mut request = DeviceReplicaRequest {
            schema_version: 1,
            nonce: Uuid::new_v4(),
            device: Uuid::new_v4(),
            scope: scope.clone(),
            catalog_revision: 1,
            after: 0,
            through: Some(head),
            limit: DEVICE_REPLICA_PAGE_LIMIT,
        };
        initialize_device_projection(&mut tx, subscription, request.device, &scope)
            .await
            .unwrap();
        while request.after < head {
            let page = device_replica_page(&mut tx, &request).await.unwrap();
            assert_eq!(
                apply_device_replica_page(&mut tx, subscription, &page)
                    .await
                    .unwrap(),
                page.next
            );
            request.after = page.next;
            request.nonce = Uuid::new_v4();
        }
        tx.commit().await.unwrap();

        sqlx::query("UPDATE memory_relations SET origin='derived' WHERE id=$1")
            .bind(reclassified)
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM memory_relations WHERE id=$1")
            .bind(removed)
            .execute(&db.pool)
            .await
            .unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        assert_eq!(
            refresh_device_replica(&mut tx, &scope).await.unwrap(),
            head + 2
        );
        request.through = None;
        let delta = device_replica_page(&mut tx, &request).await.unwrap();
        assert_eq!(delta.entries.len(), 2);
        assert!(delta.entries.iter().all(|entry| entry.record.is_none()));
        let mut removed_keys = delta
            .entries
            .iter()
            .map(|entry| entry.key.clone())
            .collect::<Vec<_>>();
        removed_keys.sort();
        let mut expected_keys = expected
            .iter()
            .map(|id| format!("relation:{id}"))
            .collect::<Vec<_>>();
        expected_keys.sort();
        assert_eq!(removed_keys, expected_keys);
        assert_eq!(
            apply_device_replica_page(&mut tx, subscription, &delta)
                .await
                .unwrap(),
            head + 2
        );
        assert_eq!(
            apply_device_replica_page(&mut tx, subscription, &delta)
                .await
                .unwrap(),
            head + 2
        );
        assert_eq!(
            refresh_device_replica(&mut tx, &scope).await.unwrap(),
            head + 2
        );
        let projected_relations: i64 = sqlx::query_scalar("SELECT count(*) FROM device_replica_records WHERE subscription=$1 AND record->>'kind'='relation'")
            .bind(subscription).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(projected_relations, 0);
        tx.commit().await.unwrap();
        // The limit still fails closed for an oversized authoritative feed.
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("UPDATE memory_relations SET origin='asserted' WHERE origin='derived'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(matches!(
            curated_project_records(&mut tx, project).await,
            Err(StoreError::Limits)
        ));
        tx.rollback().await.unwrap();
        db.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn device_replica_is_private_repeatable_and_transactional() {
        let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let project = Uuid::new_v4();
        let other = Uuid::new_v4();
        for id in [project, other] {
            sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'fixture','/private/repo')")
                .bind(id).bind(id.to_string()).execute(&db.pool).await.unwrap();
        }
        let version = Uuid::new_v4();
        let foreign = Uuid::new_v4();
        for (id, p, text) in [
            (version, project, "curated procedure"),
            (foreign, other, "OTHER_PROJECT_SENTINEL"),
        ] {
            sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,false,$3,'summary','project','project',3,0.8,now(),now(),to_tsvector('simple',$3))")
                .bind(id).bind(p).bind(text).execute(&db.pool).await.unwrap();
        }
        sqlx::query("INSERT INTO memory_sources(id,memory_entry_id,file_path,source_kind,excerpt,created_at) VALUES($1,$2,'/PRIVATE_PATH_SENTINEL','file','RAW_BODY_SENTINEL',now())")
            .bind(Uuid::new_v4()).bind(version).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO memory_relations(id,src_canonical_id,dst_canonical_id,relation_type) VALUES($1,$2,$3,'supports')")
            .bind(Uuid::new_v4()).bind(version).bind(foreign).execute(&db.pool).await.unwrap();
        let scope = ProjectScope {
            company: Uuid::new_v4(),
            area: "workshop".into(),
            owner: InstanceId::new(Uuid::new_v4()).unwrap(),
            project,
            space: SpaceId::new(Uuid::new_v4()).unwrap(),
        };
        let mut tx = db.pool.begin().await.unwrap();
        let head = refresh_device_replica(&mut tx, &scope).await.unwrap();
        assert_eq!(head, 2);
        let request = DeviceReplicaRequest {
            schema_version: 1,
            nonce: Uuid::new_v4(),
            device: Uuid::new_v4(),
            scope: scope.clone(),
            catalog_revision: 1,
            after: 0,
            through: None,
            limit: 32,
        };
        let page = device_replica_page(&mut tx, &request).await.unwrap();
        let bytes = serde_json::to_string(&page).unwrap();
        for secret in [
            "PRIVATE_PATH_SENTINEL",
            "RAW_BODY_SENTINEL",
            "OTHER_PROJECT_SENTINEL",
        ] {
            assert!(!bytes.contains(secret));
        }
        assert!(bytes.contains("unavailable_source_ids"));
        assert_eq!(refresh_device_replica(&mut tx, &scope).await.unwrap(), head);
        tx.commit().await.unwrap();
        let subscription = Uuid::new_v4();
        let mut tx = db.pool.begin().await.unwrap();
        initialize_device_projection(&mut tx, subscription, request.device, &scope)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        apply_device_replica_page(&mut tx, subscription, &page)
            .await
            .unwrap();
        tx.rollback().await.unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM device_replica_records WHERE subscription=$1")
                .bind(subscription)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        let mut tx = db.pool.begin().await.unwrap();
        assert_eq!(
            apply_device_replica_page(&mut tx, subscription, &page)
                .await
                .unwrap(),
            head
        );
        assert_eq!(
            apply_device_replica_page(&mut tx, subscription, &page)
                .await
                .unwrap(),
            head
        );
        let mut changed = page.clone();
        changed.through += 1;
        assert!(
            apply_device_replica_page(&mut tx, subscription, &changed)
                .await
                .is_err()
        );
        tx.commit().await.unwrap();
        // Removing source provenance changes just its explicit metadata, with
        // no raw bytes copied, and resumes after the old committed cursor.
        sqlx::query("DELETE FROM memory_sources WHERE memory_entry_id=$1")
            .bind(version)
            .execute(&db.pool)
            .await
            .unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        assert_eq!(
            refresh_device_replica(&mut tx, &scope).await.unwrap(),
            head + 1
        );
        let mut next = request.clone();
        next.nonce = Uuid::new_v4();
        next.after = head;
        let delta = device_replica_page(&mut tx, &next).await.unwrap();
        assert_eq!(delta.entries.len(), 1);
        apply_device_replica_page(&mut tx, subscription, &delta)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let projection = DeviceProjectionStore::open(
            db.pool.clone(),
            subscription,
            request.device,
            scope.clone(),
        )
        .await
        .unwrap();
        let hits = projection.search("procedure", 8).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].version.id, version);
        assert!(hits[0].version.unavailable_source_ids.is_empty());
        assert_eq!(projection.history(version).await.unwrap().len(), 1);
        sqlx::query("UPDATE memory_canonical_state SET status='archived',archived_at=now() WHERE canonical_id=$1").bind(version).execute(&db.pool).await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        refresh_device_replica(&mut tx, &scope).await.unwrap();
        next.after = delta.next;
        next.nonce = Uuid::new_v4();
        let archived = device_replica_page(&mut tx, &next).await.unwrap();
        apply_device_replica_page(&mut tx, subscription, &archived)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(projection.search("procedure", 8).await.unwrap().is_empty());
        assert_eq!(projection.history(version).await.unwrap().len(), 1);
        let tombstone = Uuid::new_v4();
        sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$3,2,true,'','','project','project',0,0,now(),now(),to_tsvector('simple',''))")
            .bind(tombstone).bind(project).bind(version).execute(&db.pool).await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        refresh_device_replica(&mut tx, &scope).await.unwrap();
        next.after = archived.next;
        next.nonce = Uuid::new_v4();
        let deleted = device_replica_page(&mut tx, &next).await.unwrap();
        apply_device_replica_page(&mut tx, subscription, &deleted)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let history = projection.history(version).await.unwrap();
        assert_eq!(history.len(), 2);
        assert!(history[1].tombstone);
        assert!(projection.search("procedure", 8).await.unwrap().is_empty());
        // Retention removes content but never forgets the observed owner version.
        sqlx::query(
            "DELETE FROM memory_relations WHERE src_canonical_id=$1 OR dst_canonical_id=$1",
        )
        .bind(version)
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM memory_entries WHERE canonical_id=$1")
            .bind(version)
            .execute(&db.pool)
            .await
            .unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        refresh_device_replica(&mut tx, &scope).await.unwrap();
        next.after = deleted.next;
        next.nonce = Uuid::new_v4();
        let retained = device_replica_page(&mut tx, &next).await.unwrap();
        apply_device_replica_page(&mut tx, subscription, &retained)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(projection.history(version).await.unwrap().is_empty());
        let observed:i64=sqlx::query_scalar("SELECT version FROM device_replica_owner_watermarks WHERE subscription=$1 AND canonical_id=$2").bind(subscription).bind(version).fetch_one(&db.pool).await.unwrap();
        assert_eq!(observed, 2);
        db.cleanup().await.unwrap();
    }
}
