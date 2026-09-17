// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use serde_json::{Value, json};
use std::io::Cursor;

const PRIVATE: &str = "UNRELATED-PRIVATE-CONTEXT-SENTINEL";

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn col(name: &str, data_type: &str, nullable: bool) -> Column {
    Column {
        name: name.into(),
        data_type: data_type.into(),
        nullable,
    }
}
fn reference(constraint: &str, table: &str, key: u128) -> RowReference {
    RowReference {
        constraint: constraint.into(),
        target: Some(RecordRef {
            table: table.into(),
            key: id(key).to_string(),
        }),
    }
}
fn fk(name: &str, column: &str, target: &str) -> ForeignKey {
    ForeignKey {
        name: name.into(),
        columns: vec![column.into()],
        target_table: target.into(),
        target_columns: vec!["id".into()],
    }
}
fn table_mut<'a>(inventory: &'a mut Inventory, table: &str) -> &'a mut TableInventory {
    inventory
        .tables
        .iter_mut()
        .find(|item| item.table == table)
        .expect("fixture table")
}
fn review(classification: SourceClassification) -> ClassificationReview {
    ClassificationReview {
        classification,
        source_evidence: "synthetic source reviewed".into(),
        project_evidence: "synthetic original project".into(),
        content_evidence: "synthetic content review, not human approval".into(),
    }
}

struct Fixture {
    inventory: Inventory,
    manifest: Manifest,
    payloads: BTreeMap<RecordRef, String>,
}

#[allow(clippy::too_many_arguments)]
fn push(
    inventory: &mut Inventory,
    payloads: &mut BTreeMap<RecordRef, String>,
    table: &str,
    key: u128,
    canonical: Option<u128>,
    version: Option<VersionIdentity>,
    references: Vec<RowReference>,
    payload: String,
) {
    let row = RowInventory {
        key: id(key).to_string(),
        row_sha256: digest(payload.as_bytes()),
        source_project_id: (table != "public.projects").then_some(id(1)),
        canonical_id: canonical.map(id),
        version,
        references,
    };
    payloads.insert(
        RecordRef {
            table: table.into(),
            key: row.key.clone(),
        },
        payload,
    );
    let table = table_mut(inventory, table);
    table.rows.push(row);
    table.row_count = table.rows.len() as u64;
}

fn fixture(projected: bool) -> Fixture {
    let mut inventory = Inventory {
        format_version: 1,
        source_instance: id(2),
        snapshot_id: id(3),
        captured_at: "2026-09-17T00:00:00Z".into(),
        migration_version: SUPPORTED_MIGRATION,
        projects: vec![ProjectProvenance {
            id: id(1),
            slug: "synthetic".into(),
            name: "Synthetic only".into(),
            root_path: "/synthetic".into(),
            created_at: "2026-09-17T00:00:00Z".into(),
        }],
        tables: KNOWN_TABLES
            .iter()
            .map(|(table, _)| TableInventory {
                table: (*table).into(),
                columns: vec![col("id", "uuid", false)],
                foreign_keys: vec![],
                row_count: 0,
                rows: vec![],
            })
            .collect(),
    };
    let mut payloads = BTreeMap::new();
    table_mut(&mut inventory, "public.projects").columns = vec![
        col("id", "uuid", false),
        col("slug", "text", false),
        col("name", "text", false),
        col("root_path", "text", false),
        col("created_at", "timestamp with time zone", false),
    ];
    push(&mut inventory,&mut payloads,"public.projects",1,None,None,vec![],serde_json::to_string(&json!({"id":id(1),"slug":"synthetic","name":"Synthetic only","root_path":"/synthetic","created_at":"2026-09-17T00:00:00Z"})).expect("project JSON"));
    table_mut(&mut inventory, "public.memory_entries").columns = vec![
        col("id", "uuid", false),
        col("project_id", "uuid", false),
        col("canonical_id", "uuid", false),
        col("version_no", "integer", false),
        col("is_tombstone", "boolean", false),
        col("canonical_text", "text", false),
    ];
    for (key, number, tombstone) in [(10, 1, false), (11, 2, true)] {
        push(&mut inventory,&mut payloads,"public.memory_entries",key,Some(10),Some(VersionIdentity { version_no:number,is_tombstone:tombstone }),vec![],serde_json::to_string(&json!({"id":id(key),"project_id":id(1),"canonical_id":id(10),"version_no":number,"is_tombstone":tombstone,"canonical_text":if tombstone {""}else{"Synthetic retained fact"}})).expect("version JSON"));
    }
    table_mut(&mut inventory, "public.memory_canonical_state").columns = vec![
        col("canonical_id", "uuid", false),
        col("project_id", "uuid", false),
    ];
    push(
        &mut inventory,
        &mut payloads,
        "public.memory_canonical_state",
        10,
        Some(10),
        None,
        vec![],
        json!({"canonical_id":id(10),"project_id":id(1)}).to_string(),
    );
    table_mut(&mut inventory, "public.sessions").columns = vec![
        col("id", "uuid", false),
        col("project_id", "uuid", false),
        col("context", "text", false),
    ];
    push(
        &mut inventory,
        &mut payloads,
        "public.sessions",
        20,
        None,
        None,
        vec![],
        json!({"id":id(20),"project_id":id(1),"context":PRIVATE}).to_string(),
    );
    table_mut(&mut inventory, "public.tasks").columns = vec![
        col("id", "uuid", false),
        col("session_id", "uuid", false),
        col("user_prompt", "text", false),
    ];
    table_mut(&mut inventory, "public.tasks").foreign_keys =
        vec![fk("task_session", "session_id", "public.sessions")];
    push(
        &mut inventory,
        &mut payloads,
        "public.tasks",
        21,
        None,
        None,
        vec![reference("task_session", "public.sessions", 20)],
        json!({"id":id(21),"session_id":id(20),"user_prompt":PRIVATE}).to_string(),
    );
    table_mut(&mut inventory, "public.raw_captures").columns = vec![
        col("id", "uuid", false),
        col("task_id", "uuid", false),
        col("payload_json", "jsonb", false),
    ];
    table_mut(&mut inventory, "public.raw_captures").foreign_keys =
        vec![fk("capture_task", "task_id", "public.tasks")];
    let exact = format!(
        "{{\"id\":\"{}\", \"task_id\":\"{}\", \"payload_json\":{{\"huge\":123456789012345678901234567890.12345000000000000000,\"exponent\":1e1000,\"private\":\"{PRIVATE}\"}}}}",
        id(22),
        id(21)
    );
    push(
        &mut inventory,
        &mut payloads,
        "public.raw_captures",
        22,
        None,
        None,
        vec![reference("capture_task", "public.tasks", 21)],
        exact,
    );
    table_mut(&mut inventory, "public.memory_sources").columns = vec![
        col("id", "uuid", false),
        col("memory_entry_id", "uuid", false),
        col("task_id", "uuid", true),
        col("file_path", "text", true),
        col("git_commit", "text", true),
        col("source_kind", "text", false),
        col("excerpt", "text", true),
        col("created_at", "timestamp with time zone", false),
        col("symbol_name", "text", true),
        col("symbol_kind", "text", true),
        col("line_start", "integer", true),
        col("line_end", "integer", true),
        col("content_hash", "text", true),
        col("target_memory_id", "uuid", true),
    ];
    table_mut(&mut inventory, "public.memory_sources").foreign_keys = vec![
        fk("source_version", "memory_entry_id", "public.memory_entries"),
        fk("source_task", "task_id", "public.tasks"),
    ];
    push(&mut inventory,&mut payloads,"public.memory_sources",23,Some(10),None,vec![reference("source_version","public.memory_entries",10),reference("source_task","public.tasks",21),RowReference { constraint:"logical:target_memory_id".into(),target:None }],json!({"id":id(23),"memory_entry_id":id(10),"task_id":id(21),"file_path":PRIVATE,"git_commit":PRIVATE,"source_kind":"task","excerpt":PRIVATE,"created_at":"2026-09-17T00:00:00.123456+00:00","symbol_name":PRIVATE,"symbol_kind":PRIVATE,"line_start":1,"line_end":7,"content_hash":PRIVATE,"target_memory_id":null}).to_string());
    table_mut(&mut inventory, "public.memory_source_checks").columns = vec![
        col("id", "bigint", false),
        col("source_id", "uuid", false),
        col("reason", "text", true),
    ];
    table_mut(&mut inventory, "public.memory_source_checks").foreign_keys =
        vec![fk("check_source", "source_id", "public.memory_sources")];
    for key in [24_u128, 9_007_199_254_740_993] {
        push(
            &mut inventory,
            &mut payloads,
            "public.memory_source_checks",
            key,
            Some(10),
            None,
            vec![reference("check_source", "public.memory_sources", 23)],
            json!({"id":key as i64,"source_id":id(23),"reason":PRIVATE}).to_string(),
        );
        let old = RecordRef {
            table: "public.memory_source_checks".into(),
            key: id(key).to_string(),
        };
        let payload = payloads.remove(&old).expect("source check payload");
        payloads.insert(
            RecordRef {
                table: old.table,
                key: key.to_string(),
            },
            payload,
        );
        table_mut(&mut inventory, "public.memory_source_checks")
            .rows
            .last_mut()
            .expect("source check row")
            .key = key.to_string();
    }
    for name in [
        "public.memory_entries",
        "public.memory_canonical_state",
        "public.sessions",
    ] {
        let table = table_mut(&mut inventory, name);
        table
            .foreign_keys
            .push(fk("project_fk", "project_id", "public.projects"));
        for row in &mut table.rows {
            row.references
                .push(reference("project_fk", "public.projects", 1));
        }
    }
    let mut manifest = draft_manifest(&inventory).expect("draft fixture");
    manifest.lineages[0].disposition = Disposition::Area {
        area: AreaId::General,
        review: review(SourceClassification::Knowledge),
    };
    for record in &mut manifest.records {
        record.disposition = if projected {
            Disposition::ProtectedArchive {
                reason: "mixed fixture archive".into(),
                review: review(SourceClassification::Mixed),
            }
        } else {
            Disposition::Area {
                area: AreaId::General,
                review: review(SourceClassification::Knowledge),
            }
        };
    }
    manifest.projects.push(ProjectBinding {
        area: AreaId::General,
        source: inventory.projects[0].clone(),
        reason: "preserve original fixture project".into(),
    });
    if projected {
        manifest.format_version = PROVENANCE_MANIFEST_VERSION;
        let bound = |table: &str, key: u128| {
            let record = RecordRef {
                table: table.into(),
                key: id(key).to_string(),
            };
            HashedRecordRef {
                row_sha256: digest(payloads[&record].as_bytes()),
                record,
            }
        };
        let mut projection = ProvenanceProjection {
            format_version: 1,
            decision_id: id(30),
            binding: ProvenanceBinding {
                source_instance: inventory.source_instance,
                snapshot_id: inventory.snapshot_id,
                inventory_sha256: manifest.inventory_sha256.clone(),
                destination_area: AreaId::General,
                canonical_id: id(10),
                memory_version: bound("public.memory_entries", 10),
                version: VersionIdentity {
                    version_no: 1,
                    is_tombstone: false,
                },
                source: bound("public.memory_sources", 23),
                task_constraint: "source_task".into(),
                archived_task: bound("public.tasks", 21),
            },
            content: ProjectedSourceContent::ArchiveReferenceOnly {},
            operator_attestation: OperatorReviewAttestation {
                explicitly_approved: true,
                operator: "synthetic fixture only".into(),
                approval_record_id: id(31),
                reviewed_at: "2026-09-17T00:00:00Z".into(),
                decision_sha256: String::new(),
                source_evidence: PRIVATE.into(),
                destination_evidence: "General synthetic version".into(),
                content_evidence: "Reference only; not actual human approval".into(),
            },
        };
        projection.operator_attestation.decision_sha256 = projection
            .decision_digest()
            .expect("fixture decision digest");
        manifest.provenance_projections.push(projection);
    }
    assert!(verify_manifest(&inventory, &manifest).plan_valid);
    Fixture {
        inventory,
        manifest,
        payloads,
    }
}

fn events(fixture: &Fixture) -> Vec<Value> {
    let mut output = vec![
        json!({"event":"header","format_version":1,"source_instance":fixture.inventory.source_instance,"snapshot_id":fixture.inventory.snapshot_id,"inventory_sha256":fixture.manifest.inventory_sha256,"serialization_id":SERIALIZATION_ID}),
    ];
    let mut tables: Vec<_> = fixture.inventory.tables.iter().collect();
    tables.sort_by(|a, b| a.table.cmp(&b.table));
    let mut total = 0;
    for table in tables {
        output.push(json!({"event":"table_start","table":table.table,"row_count":table.row_count}));
        let mut rows: Vec<_> = table.rows.iter().collect();
        rows.sort_by(|a, b| a.key.cmp(&b.key));
        for row in rows {
            output.push(json!({"event":"row","table":table.table,"key":row.key,"row_sha256":row.row_sha256,"row_json":fixture.payloads[&RecordRef {table:table.table.clone(),key:row.key.clone()}]}));
            total += 1;
        }
        output.push(json!({"event":"table_end","table":table.table,"row_count":table.row_count}));
    }
    output.push(
        json!({"event":"complete","table_count":fixture.inventory.tables.len(),"row_count":total}),
    );
    output
}
fn encode(events: &[Value]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for event in events {
        serde_json::to_writer(&mut bytes, event).expect("synthetic envelope");
        bytes.push(b'\n');
    }
    bytes
}
fn run(fixture: &Fixture) -> (MaterializedMigration, Vec<u8>) {
    let mut archive = Vec::new();
    let result = materialize(
        &fixture.inventory,
        &fixture.manifest,
        Cursor::new(encode(&events(fixture))),
        &mut archive,
    )
    .expect("valid synthetic materialization");
    (result, archive)
}

// Rebind synthetic evidence deliberately: these tests must reach byte/identity
// validation rather than merely fail on an obsolete digest.
fn rebind(fixture: &mut Fixture) {
    for table in &mut fixture.inventory.tables {
        for row in &mut table.rows {
            row.row_sha256 = digest(
                fixture.payloads[&RecordRef {
                    table: table.table.clone(),
                    key: row.key.clone(),
                }]
                    .as_bytes(),
            );
        }
    }
    fixture.manifest.inventory_sha256 =
        inventory_digest(&fixture.inventory).expect("synthetic digest");
    for projection in &mut fixture.manifest.provenance_projections {
        projection
            .binding
            .inventory_sha256
            .clone_from(&fixture.manifest.inventory_sha256);
        for bound in [
            &mut projection.binding.source,
            &mut projection.binding.memory_version,
            &mut projection.binding.archived_task,
        ] {
            bound.row_sha256 = digest(fixture.payloads[&bound.record].as_bytes());
        }
        projection.operator_attestation.decision_sha256 =
            projection.decision_digest().expect("synthetic decision");
    }
    assert!(verify_manifest(&fixture.inventory, &fixture.manifest).plan_valid);
}

fn change_field(fixture: &mut Fixture, table: &str, key: u128, field: &str, value: Value) {
    let payload = fixture
        .payloads
        .get_mut(&RecordRef {
            table: table.into(),
            key: if table == "public.memory_source_checks" {
                key.to_string()
            } else {
                id(key).to_string()
            },
        })
        .expect("synthetic payload");
    let mut row: Value = serde_json::from_str(payload).expect("small synthetic row");
    row[field] = value;
    *payload = row.to_string();
    rebind(fixture);
}

fn materialization_error(fixture: &Fixture) -> MaterializeError {
    materialize(
        &fixture.inventory,
        &fixture.manifest,
        Cursor::new(encode(&events(fixture))),
        &mut Vec::new(),
    )
    .err()
    .expect("malicious synthetic payload rejected")
}

#[test]
fn exact_owned_bytes_big_numbers_history_and_full_archive_are_preserved() {
    let fixture = fixture(false);
    let (materialized, archive) = run(&fixture);
    let part = materialized.partition(AreaId::General);
    assert_eq!(part.rows().len(), fixture.payloads.len());
    assert!(part.projected_sources().is_empty());
    for row in part.rows() {
        assert_eq!(row.row_json(), fixture.payloads[row.record()]);
        assert_eq!(row.source_sha256(), row.target_sha256());
        assert_eq!(row.target_sha256(), digest(row.row_json().as_bytes()));
    }
    let raw = part
        .rows()
        .iter()
        .find(|row| row.record().table == "public.raw_captures")
        .expect("exact raw fixture");
    assert!(
        raw.row_json()
            .contains("123456789012345678901234567890.12345000000000000000")
    );
    assert!(raw.row_json().contains("1e1000"));
    let checks: Vec<_> = part
        .rows()
        .iter()
        .filter(|row| row.record().table == "public.memory_source_checks")
        .collect();
    assert_eq!(
        checks.len(),
        2,
        "all append-only checks for the same source survive"
    );
    assert!(
        checks
            .iter()
            .any(|row| row.record().key == "9007199254740993"
                && row.row_json().contains("9007199254740993"))
    );
    assert_eq!(
        part.rows()
            .iter()
            .filter(|row| row.record().table == "public.memory_entries")
            .count(),
        2
    );
    assert_eq!(
        materialized.source_row_count(),
        fixture.payloads.len() as u64
    );
    assert_eq!(materialized.archive_sha256(), digest(&archive));
    assert_eq!(materialized.archive_bytes(), archive.len() as u64);
    let lines: Vec<Value> = archive
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).expect("archive envelope"))
        .collect();
    assert_eq!(
        lines
            .iter()
            .filter(|line| line["event"] == "original_row")
            .count(),
        fixture.payloads.len()
    );
    assert_eq!(
        lines
            .iter()
            .filter(|line| line["event"] == "table_start")
            .count(),
        fixture.inventory.tables.len()
    );
    assert_eq!(lines.last().expect("footer")["event"], "complete");
    assert!(materialized.partition(AreaId::Trust).rows().is_empty());
}

#[test]
fn reviewed_projection_strips_all_original_context_and_archives_originals_checks() {
    let mut fixture = fixture(true);
    for excerpt in [
        None,
        Some("Only the explicitly reviewed synthetic fact.\nUnicode: λ"),
    ] {
        let projection = &mut fixture.manifest.provenance_projections[0];
        projection.content = match excerpt {
            None => ProjectedSourceContent::ArchiveReferenceOnly {},
            Some(text) => ProjectedSourceContent::ReviewedExcerpt {
                text: text.into(),
                text_sha256: digest(text.as_bytes()),
            },
        };
        projection.operator_attestation.decision_sha256 =
            projection.decision_digest().expect("reviewed digest");
        let original_source_sha256 = projection.binding.source.row_sha256.clone();
        let (materialized, archive) = run(&fixture);
        let part = materialized.partition(AreaId::General);
        assert_eq!(part.rows().len(), 4);
        assert_eq!(part.projected_sources().len(), 1);
        assert!(
            !serde_json::to_string(part)
                .expect("protected area instructions")
                .contains(PRIVATE)
        );
        assert!(
            String::from_utf8(archive)
                .expect("UTF8 archive")
                .contains(PRIVATE)
        );
        let projected = &part.projected_sources()[0];
        let row: Value = serde_json::from_str(projected.target_row_json()).expect("projected row");
        assert_eq!(row["id"], id(23).to_string());
        assert_eq!(row["memory_entry_id"], id(10).to_string());
        assert_eq!(row["source_kind"], "note");
        assert_eq!(row["created_at"], "2026-09-17T00:00:00.123456+00:00");
        assert_eq!(
            row["excerpt"],
            excerpt.map_or(Value::Null, |text| json!(text))
        );
        for field in [
            "task_id",
            "file_path",
            "git_commit",
            "symbol_name",
            "symbol_kind",
            "line_start",
            "line_end",
            "content_hash",
            "target_memory_id",
        ] {
            assert!(row[field].is_null());
        }
        assert_eq!(projected.source_sha256(), original_source_sha256);
        assert_ne!(projected.source_sha256(), projected.target_sha256());
        assert_eq!(
            projected.target_sha256(),
            digest(projected.target_row_json().as_bytes())
        );
    }
}

#[test]
fn ordering_complete_accounting_and_header_bindings_fail_closed() {
    let fixture = fixture(false);
    let original = events(&fixture);
    let row = original
        .iter()
        .position(|event| event["event"] == "row")
        .expect("row event");
    let mut cases = Vec::new();
    let mut changed = original.clone();
    changed.remove(row);
    cases.push(changed);
    let mut changed = original.clone();
    changed.insert(row, changed[row].clone());
    cases.push(changed);
    let mut changed = original.clone();
    changed[row]["key"] = json!(PRIVATE);
    cases.push(changed);
    let mut changed = original.clone();
    changed[row]["table"] = json!("public.unknown");
    cases.push(changed);
    let mut changed = original.clone();
    changed[0]["snapshot_id"] = json!(id(999));
    cases.push(changed);
    let mut changed = original.clone();
    changed[0]["serialization_id"] = json!("lossy");
    cases.push(changed);
    let mut changed = original.clone();
    changed[1]["row_count"] = json!(999);
    cases.push(changed);
    let mut changed = original.clone();
    changed.pop();
    cases.push(changed);
    let mut changed = original.clone();
    changed.push(changed[0].clone());
    cases.push(changed);
    let mut changed = original.clone();
    changed[row]["unknown_private_field"] = json!(PRIVATE);
    cases.push(changed);
    for events in cases {
        let mut archive = Vec::new();
        let error = materialize(
            &fixture.inventory,
            &fixture.manifest,
            Cursor::new(encode(&events)),
            &mut archive,
        )
        .err()
        .expect("invalid stream");
        assert!(!error.to_string().contains(PRIVATE));
    }
}

#[test]
fn row_hash_column_null_and_duplicate_keys_are_checked_without_reserializing() {
    let mut fixture = fixture(false);
    let key = RecordRef {
        table: "public.projects".into(),
        key: id(1).to_string(),
    };
    let original = fixture.payloads[&key].clone();
    let mut bad = events(&fixture);
    let event = bad
        .iter_mut()
        .find(|event| event["event"] == "row")
        .expect("row");
    event["row_json"] = json!(PRIVATE);
    assert_eq!(
        materialize(
            &fixture.inventory,
            &fixture.manifest,
            Cursor::new(encode(&bad)),
            &mut Vec::new()
        )
        .err(),
        Some(MaterializeError::HashMismatch)
    );
    for (payload, error) in [
        (
            format!("{{\"unknown_column\":\"{PRIVATE}\"}}"),
            MaterializeError::ColumnMismatch,
        ),
        (
            format!("{{\"id\":\"{}\",\"id\":\"{}\"}}", id(1), id(1)),
            MaterializeError::InvalidRow,
        ),
        (
            original.replace(&format!("\"{}\"", id(1)), "null"),
            MaterializeError::InvalidRow,
        ),
        ("[]".into(), MaterializeError::InvalidRow),
    ] {
        fixture.payloads.insert(key.clone(), payload.clone());
        table_mut(&mut fixture.inventory, "public.projects").rows[0].row_sha256 =
            digest(payload.as_bytes());
        fixture.manifest.inventory_sha256 =
            inventory_digest(&fixture.inventory).expect("updated synthetic digest");
        assert_eq!(
            materialize(
                &fixture.inventory,
                &fixture.manifest,
                Cursor::new(encode(&events(&fixture))),
                &mut Vec::new()
            )
            .err(),
            Some(error)
        );
    }
}

#[test]
fn exact_payload_identity_references_and_version_metadata_cannot_be_forged_by_rehashing() {
    for (table, key, column, value) in [
        ("public.projects", 1, "id", json!(id(999))),
        ("public.projects", 1, "root_path", json!(PRIVATE)),
        ("public.memory_entries", 10, "id", json!(id(999))),
        (
            "public.memory_canonical_state",
            10,
            "canonical_id",
            json!(id(999)),
        ),
        (
            "public.memory_sources",
            23,
            "memory_entry_id",
            json!(id(11)),
        ),
        ("public.memory_sources", 23, "task_id", Value::Null),
        (
            "public.memory_sources",
            23,
            "target_memory_id",
            json!(id(11)),
        ),
        (
            "public.memory_source_checks",
            24,
            "source_id",
            json!(id(999)),
        ),
        ("public.memory_entries", 10, "project_id", json!(id(999))),
        ("public.memory_entries", 10, "version_no", json!(2)),
        ("public.memory_entries", 10, "is_tombstone", json!(true)),
        ("public.memory_source_checks", 24, "id", json!(25)),
        ("public.memory_source_checks", 24, "id", json!(24.5)),
    ] {
        let mut fixture = fixture(false);
        change_field(&mut fixture, table, key, column, value);
        assert_eq!(
            materialization_error(&fixture),
            MaterializeError::InvalidRow
        );
    }
    let mut fixture = fixture(false);
    table_mut(&mut fixture.inventory, "public.memory_sources").rows[0]
        .references
        .iter_mut()
        .find(|reference| reference.constraint == "source_task")
        .expect("FK")
        .target = None;
    rebind(&mut fixture);
    assert_eq!(
        materialization_error(&fixture),
        MaterializeError::InvalidRow
    );
}

#[test]
fn relation_bytes_keep_the_exact_reviewed_endpoints_even_after_rehashing() {
    let mut fixture = fixture(false);
    table_mut(&mut fixture.inventory, "public.memory_relations").columns = vec![
        col("id", "uuid", false),
        col("src_canonical_id", "uuid", false),
        col("dst_canonical_id", "uuid", false),
        col("relation_type", "text", false),
    ];
    push(&mut fixture.inventory, &mut fixture.payloads, "public.memory_relations", 50, None, None,
        vec![reference("logical:src_canonical_id", "public.memory_canonical_state", 10), reference("logical:dst_canonical_id", "public.memory_canonical_state", 10)],
        json!({"id":id(50),"src_canonical_id":id(10),"dst_canonical_id":id(10),"relation_type":"supports"}).to_string());
    table_mut(&mut fixture.inventory, "public.memory_relations").rows[0].source_project_id = None;
    fixture.manifest.format_version = PROVENANCE_MANIFEST_VERSION;
    fixture.manifest.records.push(RecordDecision {
        record: RecordRef {
            table: "public.memory_relations".into(),
            key: id(50).to_string(),
        },
        disposition: Disposition::Area {
            area: AreaId::General,
            review: review(SourceClassification::Knowledge),
        },
    });
    rebind(&mut fixture);
    let (migration, _) = run(&fixture);
    assert!(
        migration
            .partition(AreaId::General)
            .rows()
            .iter()
            .any(|row| row.record().table == "public.memory_relations")
    );
    for column in ["id", "src_canonical_id", "dst_canonical_id"] {
        change_field(
            &mut fixture,
            "public.memory_relations",
            50,
            column,
            json!(id(999)),
        );
        assert_eq!(
            materialization_error(&fixture),
            MaterializeError::InvalidRow
        );
        change_field(
            &mut fixture,
            "public.memory_relations",
            50,
            column,
            json!(id(if column == "id" { 50 } else { 10 })),
        );
    }
}

#[test]
fn claimed_version_identity_fields_cannot_be_removed_from_both_payload_and_column_catalog() {
    for column in ["canonical_id", "version_no", "is_tombstone"] {
        let mut fixture = fixture(false);
        let table = table_mut(&mut fixture.inventory, "public.memory_entries");
        table.columns.retain(|item| item.name != column);
        for row in &table.rows {
            let payload = fixture
                .payloads
                .get_mut(&RecordRef {
                    table: table.table.clone(),
                    key: row.key.clone(),
                })
                .expect("version payload");
            let mut value: Value = serde_json::from_str(payload).expect("version JSON");
            value.as_object_mut().expect("object").remove(column);
            *payload = value.to_string();
        }
        rebind(&mut fixture);
        assert_eq!(
            materialization_error(&fixture),
            MaterializeError::InvalidRow
        );
    }
}

#[test]
fn inherited_project_metadata_and_omitted_required_foreign_keys_fail_closed() {
    for name in ["public.tasks", "public.raw_captures"] {
        let mut fixture = fixture(false);
        let other = ProjectProvenance {
            id: id(50),
            slug: "synthetic-other".into(),
            name: "Synthetic other".into(),
            root_path: "/synthetic-other".into(),
            created_at: "2026-09-17T00:00:00Z".into(),
        };
        fixture.inventory.projects.push(other.clone());
        fixture.manifest.projects.push(ProjectBinding {
            area: AreaId::General,
            source: other.clone(),
            reason: "synthetic false metadata test".into(),
        });
        push(
            &mut fixture.inventory,
            &mut fixture.payloads,
            "public.projects",
            50,
            None,
            None,
            vec![],
            serde_json::to_string(&other).expect("synthetic project"),
        );
        table_mut(&mut fixture.inventory, name).rows[0].source_project_id = Some(id(50));
        rebind(&mut fixture);
        assert_eq!(
            materialization_error(&fixture),
            MaterializeError::InvalidRow
        );
    }
    let mut fixture = fixture(false);
    let source = table_mut(&mut fixture.inventory, "public.memory_sources");
    source.foreign_keys.retain(|fk| fk.name != "source_version");
    source.rows[0]
        .references
        .retain(|reference| reference.constraint != "source_version");
    rebind(&mut fixture);
    assert_eq!(
        materialization_error(&fixture),
        MaterializeError::InvalidRow
    );
}

#[test]
fn composite_tag_keys_use_exact_postgresql_array_string_encoding() {
    let mut fixture = fixture(false);
    let tag = "quoted \"tag\", newline\nλ";
    let key = format!(
        "[\"{}\", {}]",
        id(10),
        serde_json::to_string(tag).expect("tag")
    );
    let record = RecordRef {
        table: "public.memory_tags".into(),
        key: key.clone(),
    };
    let payload = json!({"memory_entry_id":id(10),"tag":tag}).to_string();
    fixture.payloads.insert(record.clone(), payload);
    let tags = table_mut(&mut fixture.inventory, "public.memory_tags");
    tags.columns = vec![
        col("memory_entry_id", "uuid", false),
        col("tag", "text", false),
    ];
    tags.foreign_keys = vec![fk(
        "tag_version",
        "memory_entry_id",
        "public.memory_entries",
    )];
    tags.row_count = 1;
    tags.rows.push(RowInventory {
        key,
        row_sha256: String::new(),
        source_project_id: Some(id(1)),
        canonical_id: Some(id(10)),
        version: None,
        references: vec![reference("tag_version", "public.memory_entries", 10)],
    });
    rebind(&mut fixture);
    let (materialized, _) = run(&fixture);
    assert!(
        materialized
            .partition(AreaId::General)
            .rows()
            .iter()
            .any(|row| row.record() == &record)
    );
    fixture.payloads.insert(
        record,
        json!({"memory_entry_id":id(10),"tag":"different"}).to_string(),
    );
    rebind(&mut fixture);
    assert_eq!(
        materialization_error(&fixture),
        MaterializeError::InvalidRow
    );
}

#[test]
fn projection_refuses_unreviewed_source_schema_and_actual_nonnull_logical_targets() {
    let mut changed = fixture(true);
    change_field(
        &mut changed,
        "public.memory_sources",
        23,
        "target_memory_id",
        json!(id(11)),
    );
    assert_eq!(
        materialization_error(&changed),
        MaterializeError::InvalidRow
    );
    let mut changed = fixture(true);
    table_mut(&mut changed.inventory, "public.memory_sources")
        .columns
        .push(col("future_context", "text", true));
    change_field(
        &mut changed,
        "public.memory_sources",
        23,
        "future_context",
        json!(PRIVATE),
    );
    assert_eq!(
        materialization_error(&changed),
        MaterializeError::UnsupportedProjection
    );
}

#[test]
fn opaque_results_are_deterministic_and_bind_area_manifest_snapshot_and_projection() {
    let mut fixture = fixture(true);
    let (first, archive) = run(&fixture);
    let (second, archive2) = run(&fixture);
    assert_eq!(archive, archive2);
    let original = first
        .partition(AreaId::General)
        .partition_sha256()
        .to_owned();
    assert_eq!(
        original,
        second.partition(AreaId::General).partition_sha256()
    );
    assert_ne!(original, first.partition(AreaId::Trust).partition_sha256());
    fixture.manifest.projects[0]
        .reason
        .push_str(" updated explicit evidence");
    let (changed, _) = run(&fixture);
    assert_ne!(
        original,
        changed.partition(AreaId::General).partition_sha256()
    );
    assert_eq!(
        changed.partition(AreaId::General).manifest_sha256(),
        manifest_digest(&fixture.manifest).expect("manifest digest")
    );
}

#[test]
fn bounded_buffers_and_sink_input_failures_are_sanitized_and_return_no_object() {
    let fixture = fixture(false);
    let payload = encode(&events(&fixture));
    assert_eq!(
        materialize_bounded(
            &fixture.inventory,
            &fixture.manifest,
            Cursor::new(&payload),
            &mut Vec::new(),
            8,
            MAX_RETAINED_TARGET_BYTES
        )
        .err(),
        Some(MaterializeError::EnvelopeTooLarge)
    );
    assert_eq!(
        materialize_bounded(
            &fixture.inventory,
            &fixture.manifest,
            Cursor::new(&payload),
            &mut Vec::new(),
            MAX_PAYLOAD_ENVELOPE_BYTES,
            8
        )
        .err(),
        Some(MaterializeError::TargetLimit)
    );
    struct Fail;
    impl Write for Fail {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other(PRIVATE))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other(PRIVATE))
        }
    }
    let error = materialize(
        &fixture.inventory,
        &fixture.manifest,
        Cursor::new(&payload),
        &mut Fail,
    )
    .err()
    .expect("sink failed");
    assert_eq!(error, MaterializeError::ArchiveIo);
    assert!(!error.to_string().contains(PRIVATE));
    struct BadRead;
    impl std::io::Read for BadRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other(PRIVATE))
        }
    }
    impl BufRead for BadRead {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            Err(std::io::Error::other(PRIVATE))
        }
        fn consume(&mut self, _: usize) {}
    }
    assert_eq!(
        materialize(
            &fixture.inventory,
            &fixture.manifest,
            BadRead,
            &mut Vec::new()
        )
        .err(),
        Some(MaterializeError::InputIo)
    );
}

#[test]
fn nonnull_memory_target_blocks_even_when_target_belongs_to_same_area() {
    let mut fixture = fixture(true);
    let source = &mut table_mut(&mut fixture.inventory, "public.memory_sources").rows[0];
    source
        .references
        .iter_mut()
        .find(|reference| reference.constraint == "logical:target_memory_id")
        .expect("logical target")
        .target = Some(RecordRef {
        table: "public.memory_entries".into(),
        key: id(11).to_string(),
    });
    fixture.manifest.inventory_sha256 =
        inventory_digest(&fixture.inventory).expect("updated fixture digest");
    let projection = &mut fixture.manifest.provenance_projections[0];
    projection.binding.inventory_sha256 = fixture.manifest.inventory_sha256.clone();
    projection.operator_attestation.decision_sha256 = projection
        .decision_digest()
        .expect("updated fixture attestation");
    let report = verify_manifest(&fixture.inventory, &fixture.manifest);
    assert!(!report.plan_valid);
    assert!(
        report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("non-null memory target"))
    );
    let mut archive = Vec::new();
    assert_eq!(
        materialize(
            &fixture.inventory,
            &fixture.manifest,
            Cursor::new(encode(&events(&fixture))),
            &mut archive
        )
        .err(),
        Some(MaterializeError::InvalidPlan)
    );
    assert!(archive.is_empty());
}
