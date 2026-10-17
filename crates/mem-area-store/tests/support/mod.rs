// SPDX-License-Identifier: AGPL-3.0-or-later
//! Synthetic PostgreSQL-to-materializer fixture. Never reads an existing source.

use std::{collections::BTreeMap, io::Cursor};

use mem_area::*;
use mem_test_support::IsolatedDatabase;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub const PRIVATE: &str = "SYNTHETIC_PRIVATE_ORIGINAL_NOT_FOR_TARGET";
pub const TARGET: Uuid = Uuid::from_u128(20000);
pub const OPERATION: Uuid = Uuid::from_u128(30000);
pub const OWNED: &[&str] = &[
    "public.projects",
    "public.sessions",
    "public.tasks",
    "public.raw_captures",
    "public.memory_entries",
    "public.memory_canonical_state",
    "public.memory_sources",
    "public.memory_source_checks",
    "public.memory_tags",
    "public.memory_relations",
    "public.project_commits",
];

pub fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
pub fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

pub async fn database() -> Option<IsolatedDatabase> {
    // Source snapshots remain explicitly supported only through migration65.
    // Start genuinely at that head; never strip modern tables or fake a head.
    IsolatedDatabase::new_through_migration(65).await
}

pub async fn execute(pool: &PgPool, sql: &str) {
    sqlx::raw_sql(sql)
        .execute(pool)
        .await
        .expect("synthetic fixture SQL");
}

pub async fn seed(pool: &PgPool) {
    sqlx::query("INSERT INTO projects(id,slug,name,root_path,created_at) VALUES($1,'synthetic-area','Synthetic area','/synthetic/area','2026-09-01T10:00:00.123456Z')")
        .bind(id(1)).execute(pool).await.expect("synthetic fixture operation");
    sqlx::query("INSERT INTO sessions(id,project_id,external_session_id,started_at,ended_at,writer_name,writer_id) VALUES($1,$2,'synthetic-session','2026-09-01T10:01:00Z','2026-09-01T11:00:00Z','synthetic-author','synthetic-writer')")
        .bind(id(2)).bind(id(1)).execute(pool).await.expect("synthetic fixture operation");
    sqlx::query("INSERT INTO tasks(id,session_id,title,user_prompt,task_summary,status,created_at,completed_at) VALUES($1,$2,'Synthetic task',$3,'Synthetic summary','complete','2026-09-01T10:02:00Z','2026-09-01T10:59:00Z')")
        .bind(id(3)).bind(id(2)).bind(PRIVATE).execute(pool).await.expect("synthetic fixture operation");
    sqlx::query("INSERT INTO raw_captures(id,task_id,capture_type,payload_json,idempotency_key,created_at) VALUES($1,$2,'synthetic',$3,'synthetic-capture','2026-09-01T10:03:00Z')")
        .bind(id(4)).bind(id(3)).bind(serde_json::json!({"private": PRIVATE, "nested": [1, true, "é"]})).execute(pool).await.expect("synthetic fixture operation");
    // IDs deliberately sort in reverse history order. The canonical-state
    // insert trigger must not overwrite the captured final mutable state.
    for (row_id, version, tombstone, text) in [
        (13, 1, false, "first historical text"),
        (12, 2, false, "second historical text é"),
        (11, 3, true, ""),
    ] {
        sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$3,$4,$5,$6,$6,'documentation','project',3,0.5,'2026-09-02T10:00:00Z','2026-09-02T11:00:00.654321Z',to_tsvector('simple',$6))")
            .bind(id(row_id)).bind(id(1)).bind(id(13)).bind(version).bind(tombstone).bind(text).execute(pool).await.expect("synthetic fixture operation");
    }
    sqlx::query("UPDATE memory_canonical_state SET status='archived',archived_at='2026-09-03T10:00:00Z',confidence=0.875,importance=9,updated_at='2026-09-03T10:01:00.123456Z' WHERE canonical_id=$1")
        .bind(id(13)).execute(pool).await.expect("synthetic fixture operation");
    sqlx::query("INSERT INTO memory_sources(id,memory_entry_id,task_id,file_path,git_commit,source_kind,excerpt,created_at,symbol_name,symbol_kind,line_start,line_end,content_hash) VALUES($1,$2,$3,'/synthetic/private-source','synthetic-commit','file',$4,'2026-09-02T12:00:00Z','synthetic_symbol','function',7,19,'synthetic-content-hash')")
        .bind(id(20)).bind(id(12)).bind(id(3)).bind(PRIVATE).execute(pool).await.expect("synthetic fixture operation");
    for (row_id, status) in [(41_i64, "valid"), (42, "stale")] {
        sqlx::query("INSERT INTO memory_source_checks(id,source_id,status,checked_at,reason,resolved_path,observed_git_commit,observed_content_hash) OVERRIDING SYSTEM VALUE VALUES($1,$2,$3,'2026-09-03T12:00:00Z',$4,'/synthetic/private-source','synthetic-commit','synthetic-content-hash')")
            .bind(row_id).bind(id(20)).bind(status).bind(PRIVATE).execute(pool).await.expect("synthetic fixture operation");
    }
    sqlx::query(
        "INSERT INTO memory_tags(memory_entry_id,tag) VALUES($1,'history'),($2,'reviewed é')",
    )
    .bind(id(13))
    .bind(id(12))
    .execute(pool)
    .await
    .expect("synthetic fixture operation");
    sqlx::query("INSERT INTO project_commits(id,project_id,commit_hash,short_hash,subject,body,author_name,author_email,committed_at,parent_hashes,changed_paths,imported_at,search_document) VALUES($1,$2,'synthetic-commit','synthet','Synthetic commit','Synthetic body','Synthetic Author','synthetic@example.invalid','2026-09-03T14:00:00Z',ARRAY['synthetic-parent'],ARRAY['synthetic.rs'],'2026-09-03T15:00:00Z',to_tsvector('simple','Synthetic commit'))")
        .bind(id(31)).bind(id(1)).execute(pool).await.expect("synthetic fixture operation");
}

#[derive(Deserialize)]
struct Catalog {
    table: String,
    columns: Vec<Column>,
    primary_key: Vec<String>,
    foreign_keys: Vec<ForeignKey>,
}

struct Captured {
    key: String,
    bytes: String,
    value: Value,
}

fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn uuid_field(row: &Value, field: &str) -> Option<Uuid> {
    row.get(field)
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
}

fn lookup<'a>(data: &'a BTreeMap<String, Vec<Captured>>, table: &str, id: Uuid) -> &'a Value {
    &data[table]
        .iter()
        .find(|row| uuid_field(&row.value, "id") == Some(id))
        .expect("fixture reference")
        .value
}

fn ownership(
    data: &BTreeMap<String, Vec<Captured>>,
    table: &str,
    row: &Value,
) -> (Option<Uuid>, Option<Uuid>) {
    let owner = match table {
        "public.tasks" => lookup(
            data,
            "public.sessions",
            uuid_field(row, "session_id").expect("synthetic fixture operation"),
        ),
        "public.raw_captures" => {
            let task = lookup(
                data,
                "public.tasks",
                uuid_field(row, "task_id").expect("synthetic fixture operation"),
            );
            lookup(
                data,
                "public.sessions",
                uuid_field(task, "session_id").expect("synthetic fixture operation"),
            )
        }
        "public.memory_sources" | "public.memory_tags" => lookup(
            data,
            "public.memory_entries",
            uuid_field(row, "memory_entry_id").expect("synthetic fixture operation"),
        ),
        "public.memory_source_checks" => {
            let source = lookup(
                data,
                "public.memory_sources",
                uuid_field(row, "source_id").expect("synthetic fixture operation"),
            );
            lookup(
                data,
                "public.memory_entries",
                uuid_field(source, "memory_entry_id").expect("synthetic fixture operation"),
            )
        }
        _ => row,
    };
    (
        uuid_field(owner, "project_id"),
        uuid_field(owner, "canonical_id"),
    )
}

pub struct Fixture {
    pub inventory: Inventory,
    pub manifest: Manifest,
    pub payload: Vec<u8>,
}

pub async fn capture(pool: &PgPool) -> Fixture {
    let mut tx = pool.begin().await.expect("synthetic fixture operation");
    sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY; SET LOCAL timezone='UTC'; SET LOCAL datestyle='ISO, YMD'; SET LOCAL intervalstyle='postgres'; SET LOCAL extra_float_digits=3; SET LOCAL bytea_output='hex'; SET LOCAL search_path=pg_catalog,public;").execute(&mut *tx).await.expect("synthetic fixture operation");
    let catalog: Value = sqlx::query_scalar(r#"SELECT jsonb_agg(t ORDER BY t.table) FROM (
        SELECT n.nspname||'.'||c.relname AS table,
        (SELECT jsonb_agg(jsonb_build_object('name',a.attname,'data_type',format_type(a.atttypid,a.atttypmod),'nullable',NOT a.attnotnull) ORDER BY a.attnum) FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attnum>0 AND NOT a.attisdropped) AS columns,
        (SELECT jsonb_agg(a.attname ORDER BY k.ordinality) FROM pg_constraint p CROSS JOIN LATERAL unnest(p.conkey) WITH ORDINALITY k(attnum,ordinality) JOIN pg_attribute a ON a.attrelid=c.oid AND a.attnum=k.attnum WHERE p.conrelid=c.oid AND p.contype='p') AS primary_key,
        COALESCE((SELECT jsonb_agg(jsonb_build_object('name',f.conname,
          'columns',(SELECT jsonb_agg(a.attname ORDER BY k.ordinality) FROM unnest(f.conkey) WITH ORDINALITY k(attnum,ordinality) JOIN pg_attribute a ON a.attrelid=c.oid AND a.attnum=k.attnum),
          'target_table',tn.nspname||'.'||tc.relname,
          'target_columns',(SELECT jsonb_agg(a.attname ORDER BY k.ordinality) FROM unnest(f.confkey) WITH ORDINALITY k(attnum,ordinality) JOIN pg_attribute a ON a.attrelid=tc.oid AND a.attnum=k.attnum)) ORDER BY f.conname)
          FROM pg_constraint f JOIN pg_class tc ON tc.oid=f.confrelid JOIN pg_namespace tn ON tn.oid=tc.relnamespace WHERE f.conrelid=c.oid AND f.contype='f'),'[]'::jsonb) AS foreign_keys
        FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE c.relkind IN ('r','p','f') AND n.nspname<>'information_schema' AND n.nspname !~ '^pg_') t"#)
        .fetch_one(&mut *tx).await.expect("synthetic fixture operation");
    let catalog: Vec<Catalog> =
        serde_json::from_value(catalog).expect("synthetic fixture operation");
    let mut data = BTreeMap::new();
    for table in &catalog {
        let key = if table.primary_key.len() == 1 {
            format!("to_jsonb(r)->>'{}'", table.primary_key[0])
        } else {
            assert!(!table.primary_key.is_empty());
            format!(
                "jsonb_build_array({})::text",
                table
                    .primary_key
                    .iter()
                    .map(|key| format!("r.{}", quote(key)))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let rows = sqlx::query(&format!(
            "SELECT {key} AS key,to_jsonb(r)::text AS bytes FROM ONLY {} r",
            table.table
        ))
        .fetch_all(&mut *tx)
        .await
        .expect("synthetic fixture operation");
        let mut captured = rows
            .into_iter()
            .map(|row| {
                let bytes: String = row.get("bytes");
                Captured {
                    key: row.get("key"),
                    value: serde_json::from_str(&bytes).expect("synthetic fixture operation"),
                    bytes,
                }
            })
            .collect::<Vec<_>>();
        captured.sort_by(|a, b| a.key.cmp(&b.key));
        data.insert(table.table.clone(), captured);
    }
    tx.commit().await.expect("synthetic fixture operation");
    let projects = data["public.projects"]
        .iter()
        .map(|row| {
            let v = &row.value;
            ProjectProvenance {
                id: uuid_field(v, "id").expect("synthetic fixture operation"),
                slug: v["slug"]
                    .as_str()
                    .expect("synthetic fixture operation")
                    .into(),
                name: v["name"]
                    .as_str()
                    .expect("synthetic fixture operation")
                    .into(),
                root_path: v["root_path"]
                    .as_str()
                    .expect("synthetic fixture operation")
                    .into(),
                created_at: v["created_at"]
                    .as_str()
                    .expect("synthetic fixture operation")
                    .into(),
            }
        })
        .collect();
    let tables = catalog
        .iter()
        .map(|table| {
            let rows = data[&table.table]
                .iter()
                .map(|row| {
                    let mut references = table
                        .foreign_keys
                        .iter()
                        .map(|fk| {
                            let target =
                                if fk.columns.iter().any(|column| row.value[column].is_null()) {
                                    None
                                } else {
                                    let target = data[&fk.target_table]
                                        .iter()
                                        .find(|target| {
                                            fk.columns.iter().zip(&fk.target_columns).all(
                                                |(from, to)| row.value[from] == target.value[to],
                                            )
                                        })
                                        .expect("catalog FK target");
                                    Some(RecordRef {
                                        table: fk.target_table.clone(),
                                        key: target.key.clone(),
                                    })
                                };
                            RowReference {
                                constraint: fk.name.clone(),
                                target,
                            }
                        })
                        .collect::<Vec<_>>();
                    for (column, target) in match table.table.as_str() {
                        "public.memory_sources" => {
                            vec![("target_memory_id", "public.memory_entries")]
                        }
                        "public.memory_relations" => vec![
                            ("src_canonical_id", "public.memory_canonical_state"),
                            ("dst_canonical_id", "public.memory_canonical_state"),
                        ],
                        _ => vec![],
                    } {
                        references.push(RowReference {
                            constraint: format!("logical:{column}"),
                            target: uuid_field(&row.value, column).map(|id| RecordRef {
                                table: target.into(),
                                key: id.to_string(),
                            }),
                        });
                    }
                    let (source_project_id, canonical_id) =
                        ownership(&data, &table.table, &row.value);
                    RowInventory {
                        key: row.key.clone(),
                        row_sha256: hash(&row.bytes),
                        source_project_id,
                        canonical_id,
                        version: (table.table == "public.memory_entries").then(|| {
                            VersionIdentity {
                                version_no: row.value["version_no"]
                                    .as_u64()
                                    .expect("synthetic fixture operation")
                                    as u32,
                                is_tombstone: row.value["is_tombstone"]
                                    .as_bool()
                                    .expect("synthetic fixture operation"),
                            }
                        }),
                        references,
                    }
                })
                .collect::<Vec<_>>();
            TableInventory {
                table: table.table.clone(),
                columns: table.columns.clone(),
                foreign_keys: table.foreign_keys.clone(),
                row_count: rows.len() as u64,
                rows,
            }
        })
        .collect();
    let inventory = Inventory {
        format_version: FORMAT_VERSION,
        source_instance: id(10000),
        snapshot_id: id(10001),
        captured_at: "2026-09-17T12:00:00Z".into(),
        migration_version: sqlx::query_scalar::<_, i64>(
            "SELECT max(version) FROM _sqlx_migrations WHERE success",
        )
        .fetch_one(pool)
        .await
        .expect("synthetic schema head") as u32,
        projects,
        tables,
    };
    assert!(
        verify_inventory(&inventory).is_empty(),
        "{:?}",
        verify_inventory(&inventory)
    );
    let mut manifest = draft_manifest(&inventory).expect("synthetic fixture operation");
    for lineage in &mut manifest.lineages {
        lineage.disposition = area();
    }
    for record in &mut manifest.records {
        record.disposition = area();
    }
    for project in &inventory.projects {
        manifest.projects.push(ProjectBinding {
            area: AreaId::General,
            source: project.clone(),
            reason: "Synthetic exact project preservation".into(),
        });
    }
    let mut payload = Vec::new();
    event(
        &mut payload,
        PayloadEvent::Header {
            format_version: 1,
            source_instance: inventory.source_instance,
            snapshot_id: inventory.snapshot_id,
            inventory_sha256: inventory_digest(&inventory).expect("synthetic fixture operation"),
            serialization_id: SERIALIZATION_ID.into(),
        },
    );
    let mut row_count = 0;
    for table in &inventory.tables {
        event(
            &mut payload,
            PayloadEvent::TableStart {
                table: table.table.clone(),
                row_count: table.row_count,
            },
        );
        for row in &data[&table.table] {
            event(
                &mut payload,
                PayloadEvent::Row {
                    table: table.table.clone(),
                    key: row.key.clone(),
                    row_sha256: hash(&row.bytes),
                    row_json: row.bytes.clone(),
                },
            );
            row_count += 1;
        }
        event(
            &mut payload,
            PayloadEvent::TableEnd {
                table: table.table.clone(),
                row_count: table.row_count,
            },
        );
    }
    event(
        &mut payload,
        PayloadEvent::Complete {
            table_count: inventory.tables.len() as u64,
            row_count,
        },
    );
    Fixture {
        inventory,
        manifest,
        payload,
    }
}

fn event(payload: &mut Vec<u8>, event: PayloadEvent) {
    serde_json::to_writer(&mut *payload, &event).expect("synthetic fixture operation");
    payload.push(b'\n');
}
fn review(classification: SourceClassification) -> ClassificationReview {
    ClassificationReview {
        classification,
        source_evidence: "Synthetic source reviewed".into(),
        project_evidence: "Synthetic project reviewed".into(),
        content_evidence: "Synthetic complete content reviewed".into(),
    }
}
fn area() -> Disposition {
    Disposition::Area {
        area: AreaId::General,
        review: review(SourceClassification::Knowledge),
    }
}

impl Fixture {
    pub fn partition(&self) -> (MaterializedPartition, Vec<u8>) {
        let report = verify_manifest(&self.inventory, &self.manifest);
        assert!(report.plan_valid, "{:?}", report.blockers);
        let mut archive = Vec::new();
        let migration = materialize(
            &self.inventory,
            &self.manifest,
            Cursor::new(&self.payload),
            &mut archive,
        )
        .expect("synthetic materialization");
        (migration.partition(AreaId::General).clone(), archive)
    }

    pub fn project_provenance(&mut self) {
        self.manifest.format_version = PROVENANCE_MANIFEST_VERSION;
        for record in &mut self.manifest.records {
            if matches!(
                record.record.table.as_str(),
                "public.sessions" | "public.tasks" | "public.raw_captures"
            ) {
                record.disposition = Disposition::ProtectedArchive {
                    reason: "Synthetic mixed private context".into(),
                    review: review(SourceClassification::Mixed),
                };
            }
        }
        let bound = |table: &str, key: Uuid| {
            let row = self
                .inventory
                .tables
                .iter()
                .find(|t| t.table == table)
                .expect("synthetic fixture operation")
                .rows
                .iter()
                .find(|r| r.key == key.to_string())
                .expect("synthetic fixture operation");
            HashedRecordRef {
                record: RecordRef {
                    table: table.into(),
                    key: row.key.clone(),
                },
                row_sha256: row.row_sha256.clone(),
            }
        };
        let mut projection = ProvenanceProjection {
            format_version: PROVENANCE_PROJECTION_VERSION,
            decision_id: id(40000),
            binding: ProvenanceBinding {
                source_instance: self.inventory.source_instance,
                snapshot_id: self.inventory.snapshot_id,
                inventory_sha256: self.manifest.inventory_sha256.clone(),
                destination_area: AreaId::General,
                canonical_id: id(13),
                memory_version: bound("public.memory_entries", id(12)),
                version: VersionIdentity {
                    version_no: 2,
                    is_tombstone: false,
                },
                source: bound("public.memory_sources", id(20)),
                task_constraint: "memory_sources_task_id_fkey".into(),
                archived_task: bound("public.tasks", id(3)),
            },
            content: ProjectedSourceContent::ArchiveReferenceOnly {},
            operator_attestation: OperatorReviewAttestation {
                explicitly_approved: true,
                operator: "Synthetic operator, not real approval".into(),
                approval_record_id: id(40001),
                reviewed_at: "2026-09-17T12:00:00Z".into(),
                decision_sha256: String::new(),
                source_evidence: "Synthetic original reviewed".into(),
                destination_evidence: "Synthetic General version reviewed".into(),
                content_evidence: "Only synthetic archive reference approved".into(),
            },
        };
        projection.operator_attestation.decision_sha256 = projection
            .decision_digest()
            .expect("synthetic fixture operation");
        self.manifest.provenance_projections.push(projection);
    }
}

pub async fn rows(pool: &PgPool, table: &str) -> Vec<String> {
    assert!(
        OWNED.contains(&table)
            || matches!(
                table,
                "public.area_import_receipts"
                    | "public.area_import_rows"
                    | "public.area_import_provenance"
            )
    );
    let mut tx = pool.begin().await.expect("synthetic fixture operation");
    sqlx::raw_sql("SET LOCAL timezone='UTC'; SET LOCAL datestyle='ISO, YMD'; SET LOCAL intervalstyle='postgres'; SET LOCAL extra_float_digits=3; SET LOCAL bytea_output='hex';").execute(&mut *tx).await.expect("synthetic fixture operation");
    let values = sqlx::query_scalar(&format!(
        "SELECT to_jsonb(t)::text FROM {table} t ORDER BY to_jsonb(t)::text"
    ))
    .fetch_all(&mut *tx)
    .await
    .expect("synthetic fixture operation");
    tx.commit().await.expect("synthetic fixture operation");
    values
}

pub async fn assert_empty(pool: &PgPool) {
    for table in OWNED.iter().copied().chain([
        "public.area_import_receipts",
        "public.area_import_rows",
        "public.area_import_provenance",
    ]) {
        assert!(
            rows(pool, table).await.is_empty(),
            "unexpected rows after failed import: {table}"
        );
    }
    let sequence: (i64, bool) =
        sqlx::query_as("SELECT last_value,is_called FROM public.memory_source_checks_id_seq")
            .fetch_one(pool)
            .await
            .expect("synthetic fixture operation");
    assert_eq!(
        sequence,
        (1, false),
        "failed import must roll back sequence restart"
    );
    let kill_switch: bool = sqlx::query_scalar(
        "SELECT kill_switch_enabled FROM public.loop_global_state WHERE id=true",
    )
    .fetch_one(pool)
    .await
    .expect("synthetic fixture operation");
    assert!(
        !kill_switch,
        "failed import must roll back runtime state mutation"
    );
}
