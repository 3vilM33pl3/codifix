// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeSet;

use mem_area::*;
use uuid::Uuid;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn row(
    key: String,
    project: Option<Uuid>,
    canonical: Option<Uuid>,
    version: Option<VersionIdentity>,
) -> RowInventory {
    RowInventory {
        key,
        row_sha256: "a".repeat(64),
        source_project_id: project,
        canonical_id: canonical,
        version,
        references: vec![],
    }
}

fn table_mut<'a>(inventory: &'a mut Inventory, name: &str) -> &'a mut TableInventory {
    inventory
        .tables
        .iter_mut()
        .find(|table| table.table == name)
        .expect("fixture table")
}

fn push(inventory: &mut Inventory, name: &str, row: RowInventory) {
    let table = table_mut(inventory, name);
    table.rows.push(row);
    table.row_count = table.rows.len() as u64;
}

fn fixture() -> Inventory {
    let project = ProjectProvenance {
        id: id(1),
        slug: "ca-2".into(),
        name: "Original project".into(),
        root_path: "/synthetic/ca-2".into(),
        created_at: "2026-09-17T08:00:00Z".into(),
    };
    let mut inventory = Inventory {
        format_version: FORMAT_VERSION,
        source_instance: id(2),
        snapshot_id: id(3),
        captured_at: "2026-09-17T08:00:00Z".into(),
        migration_version: SUPPORTED_MIGRATION,
        projects: vec![project],
        tables: KNOWN_TABLES
            .iter()
            .map(|(name, _)| TableInventory {
                table: (*name).into(),
                columns: vec![Column {
                    name: "id".into(),
                    data_type: "uuid".into(),
                    nullable: false,
                }],
                foreign_keys: vec![],
                row_count: 0,
                rows: vec![],
            })
            .collect(),
    };
    push(
        &mut inventory,
        "public.projects",
        row(id(1).to_string(), None, None, None),
    );
    for (key, version_no, is_tombstone) in [(10, 1, false), (11, 2, false), (12, 3, true)] {
        push(
            &mut inventory,
            "public.memory_entries",
            row(
                id(key).to_string(),
                Some(id(1)),
                Some(id(10)),
                Some(VersionIdentity {
                    version_no,
                    is_tombstone,
                }),
            ),
        );
    }
    push(
        &mut inventory,
        "public.memory_canonical_state",
        row(id(10).to_string(), Some(id(1)), Some(id(10)), None),
    );
    inventory
}

fn review(classification: SourceClassification) -> ClassificationReview {
    ClassificationReview {
        classification,
        source_evidence: "fixture source reviewed".into(),
        project_evidence: "original project preserved".into(),
        content_evidence: "all version content reviewed".into(),
    }
}

fn reviewed(inventory: &Inventory) -> Manifest {
    let mut manifest = draft_manifest(inventory).expect("serialize fixture");
    for lineage in &mut manifest.lineages {
        lineage.disposition = Disposition::Area {
            area: AreaId::General,
            review: review(SourceClassification::Knowledge),
        };
    }
    manifest.projects.push(ProjectBinding {
        area: AreaId::General,
        source: inventory.projects[0].clone(),
        reason: "Preserve original project subdivision".into(),
    });
    manifest
}

fn assert_blocked(inventory: &Inventory, manifest: &Manifest, needle: &str) {
    let report = verify_manifest(inventory, manifest);
    assert!(!report.plan_valid, "expected blocker {needle}");
    assert!(!report.import_supported);
    assert!(
        report.records.is_empty(),
        "invalid plans cannot expose partial executable actions"
    );
    assert!(report.provenance_projections.is_empty());
    assert!(
        report
            .blockers
            .iter()
            .any(|message| message.contains(needle)),
        "missing {needle}: {:?}",
        report.blockers
    );
}

fn foreign_key(inventory: &mut Inventory, table: &str, name: &str, column: &str, target: &str) {
    let table = table_mut(inventory, table);
    table.columns.push(Column {
        name: column.into(),
        data_type: "uuid".into(),
        nullable: false,
    });
    table.foreign_keys.push(ForeignKey {
        name: name.into(),
        columns: vec![column.into()],
        target_table: target.into(),
        target_columns: vec!["id".into()],
    });
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

fn bind_record(inventory: &Inventory, table: &str, key: u128) -> HashedRecordRef {
    let row = inventory
        .tables
        .iter()
        .find(|item| item.table == table)
        .expect("synthetic fixture")
        .rows
        .iter()
        .find(|row| row.key == id(key).to_string())
        .expect("synthetic fixture");
    HashedRecordRef {
        record: RecordRef {
            table: table.into(),
            key: row.key.clone(),
        },
        row_sha256: row.row_sha256.clone(),
    }
}

fn attest(projection: &mut ProvenanceProjection) {
    // Synthetic externally supplied attestation, never produced by a draft.
    projection.operator_attestation.decision_sha256 =
        projection.decision_digest().expect("synthetic fixture");
}

fn projection_fixture() -> (Inventory, Manifest) {
    let mut inventory = fixture();
    foreign_key(
        &mut inventory,
        "public.tasks",
        "task_session",
        "session_id",
        "public.sessions",
    );
    foreign_key(
        &mut inventory,
        "public.raw_captures",
        "capture_task",
        "task_id",
        "public.tasks",
    );
    foreign_key(
        &mut inventory,
        "public.memory_sources",
        "source_version",
        "memory_entry_id",
        "public.memory_entries",
    );
    foreign_key(
        &mut inventory,
        "public.memory_sources",
        "source_task",
        "task_id",
        "public.tasks",
    );
    foreign_key(
        &mut inventory,
        "public.memory_source_checks",
        "check_source",
        "source_id",
        "public.memory_sources",
    );
    push(
        &mut inventory,
        "public.sessions",
        row(id(20).to_string(), Some(id(1)), None, None),
    );
    let mut task = row(id(21).to_string(), Some(id(1)), None, None);
    task.references
        .push(reference("task_session", "public.sessions", 20));
    push(&mut inventory, "public.tasks", task);
    let mut capture = row(id(22).to_string(), Some(id(1)), None, None);
    capture
        .references
        .push(reference("capture_task", "public.tasks", 21));
    push(&mut inventory, "public.raw_captures", capture);
    let mut source = row(id(23).to_string(), Some(id(1)), Some(id(10)), None);
    source.row_sha256 = "b".repeat(64);
    source.references = vec![
        reference("source_version", "public.memory_entries", 11),
        reference("source_task", "public.tasks", 21),
        RowReference {
            constraint: "logical:target_memory_id".into(),
            target: None,
        },
    ];
    push(&mut inventory, "public.memory_sources", source);
    let mut check = row(id(24).to_string(), Some(id(1)), Some(id(10)), None);
    check
        .references
        .push(reference("check_source", "public.memory_sources", 23));
    push(&mut inventory, "public.memory_source_checks", check);
    assert!(verify_inventory(&inventory).is_empty());
    let mut manifest = reviewed(&inventory);
    manifest.format_version = PROVENANCE_MANIFEST_VERSION;
    for decision in &mut manifest.records {
        decision.disposition = Disposition::ProtectedArchive {
            reason: "Mixed raw context remains non-retrievable".into(),
            review: review(SourceClassification::Mixed),
        };
    }
    let mut projection = ProvenanceProjection {
        format_version: PROVENANCE_PROJECTION_VERSION,
        decision_id: id(30),
        binding: ProvenanceBinding {
            source_instance: inventory.source_instance,
            snapshot_id: inventory.snapshot_id,
            inventory_sha256: manifest.inventory_sha256.clone(),
            destination_area: AreaId::General,
            canonical_id: id(10),
            memory_version: bind_record(&inventory, "public.memory_entries", 11),
            version: VersionIdentity { version_no: 2, is_tombstone: false },
            source: bind_record(&inventory, "public.memory_sources", 23),
            task_constraint: "source_task".into(),
            archived_task: bind_record(&inventory, "public.tasks", 21),
        },
        content: ProjectedSourceContent::ArchiveReferenceOnly {},
        operator_attestation: OperatorReviewAttestation {
            explicitly_approved: true,
            operator: "Synthetic operator, not a real human approval".into(),
            approval_record_id: id(31),
            reviewed_at: "2026-09-17T12:00:00Z".into(),
            decision_sha256: String::new(),
            source_evidence: "Reviewed fixture source and mixed task context".into(),
            destination_evidence: "This exact version belongs to General".into(),
            content_evidence: "Only bound reference metadata may be projected; old checks and raw context remain archived".into(),
        },
    };
    attest(&mut projection);
    manifest.provenance_projections.push(projection);
    (inventory, manifest)
}

#[test]
fn v2_reference_projection_preserves_every_original_row_and_archives_raw_context_and_checks() {
    let (inventory, manifest) = projection_fixture();
    let report = verify_manifest(&inventory, &manifest);
    assert!(report.plan_valid, "{:?}", report.blockers);
    assert!(!report.import_supported);
    assert_eq!(
        report.records.len(),
        inventory
            .tables
            .iter()
            .map(|table| table.rows.len())
            .sum::<usize>()
    );
    assert_eq!(
        report.provenance_projections,
        manifest.provenance_projections
    );
    for action in &report.records {
        let original = inventory
            .tables
            .iter()
            .find(|table| table.table == action.record.table)
            .expect("synthetic fixture")
            .rows
            .iter()
            .find(|row| row.key == action.record.key)
            .expect("synthetic fixture");
        assert_eq!(action.source_sha256, original.row_sha256);
        assert_eq!(action.source_project_id, original.source_project_id);
        assert_eq!(action.canonical_id, original.canonical_id);
        match action.record.table.as_str() {
            "public.memory_sources" => assert_eq!(
                action.destination,
                Destination::ArchiveAndProjectProvenance(AreaId::General)
            ),
            "public.tasks"
            | "public.sessions"
            | "public.raw_captures"
            | "public.memory_source_checks" => {
                assert_eq!(action.destination, Destination::ProtectedArchive)
            }
            "public.memory_entries" => {
                assert_eq!(action.destination, Destination::Area(AreaId::General))
            }
            _ => {}
        }
    }
    // Pure verification is deterministic/idempotent; no persistent consumption
    // or global replay database is claimed by this offline facility.
    assert_eq!(report, verify_manifest(&inventory, &manifest));
}

#[test]
fn v1_wire_format_and_archive_reference_rejection_remain_unchanged() {
    // Frozen pre-extension field order and shape, independent of the new type.
    #[derive(serde::Serialize)]
    struct LegacyManifest<'a> {
        format_version: u32,
        source_instance: Uuid,
        snapshot_id: Uuid,
        inventory_sha256: &'a str,
        inventory_notices: &'a [InventoryNotice],
        areas: &'a [AreaId],
        table_policies: &'a [TablePolicy],
        lineages: &'a [LineageDecision],
        records: &'a [RecordDecision],
        projects: &'a [ProjectBinding],
    }
    let inventory = fixture();
    let manifest = draft_manifest(&inventory).expect("synthetic fixture");
    let legacy = LegacyManifest {
        format_version: manifest.format_version,
        source_instance: manifest.source_instance,
        snapshot_id: manifest.snapshot_id,
        inventory_sha256: &manifest.inventory_sha256,
        inventory_notices: &manifest.inventory_notices,
        areas: &manifest.areas,
        table_policies: &manifest.table_policies,
        lineages: &manifest.lineages,
        records: &manifest.records,
        projects: &manifest.projects,
    };
    assert_eq!(
        serde_json::to_vec(&manifest).expect("v1 manifest bytes"),
        serde_json::to_vec(&legacy).expect("legacy manifest bytes")
    );
    assert_eq!(manifest.format_version, 1);
    let serialized = serde_json::to_value(&manifest).expect("synthetic fixture");
    assert!(serialized.get("provenance_projections").is_none());
    assert_eq!(
        serde_json::from_value::<Manifest>(serialized.clone()).expect("synthetic fixture"),
        manifest
    );
    let report = verify_manifest(&inventory, &reviewed(&inventory));
    assert!(
        serde_json::to_value(report)
            .expect("synthetic fixture")
            .get("provenance_projections")
            .is_none()
    );
    let (inventory, mut manifest) = projection_fixture();
    manifest.format_version = 1;
    assert_blocked(&inventory, &manifest, "require manifest format 2");
    manifest.provenance_projections.clear();
    assert_blocked(&inventory, &manifest, "requires reviewed transformation");
}

#[test]
fn v2_unknown_versions_and_projection_fields_fail_closed() {
    let (inventory, mut manifest) = projection_fixture();
    manifest.format_version = 99;
    assert_blocked(&inventory, &manifest, "format/source/snapshot mismatch");
    manifest.format_version = PROVENANCE_MANIFEST_VERSION;
    manifest.provenance_projections[0].format_version = 99;
    attest(&mut manifest.provenance_projections[0]);
    assert_blocked(
        &inventory,
        &manifest,
        "unsupported/duplicate provenance projection",
    );
    let mut json = serde_json::to_value(&manifest).expect("synthetic fixture");
    json["provenance_projections"][0]["content"]["raw_capture_payload"] =
        serde_json::json!("must not be accepted");
    assert!(serde_json::from_value::<Manifest>(json).is_err());
}

#[test]
fn projections_require_explicit_complete_operator_attestation_and_exact_decision_digest() {
    let (inventory, manifest) = projection_fixture();
    for mutate in [
        (|p: &mut ProvenanceProjection| p.operator_attestation.explicitly_approved = false)
            as fn(&mut ProvenanceProjection),
        |p| p.operator_attestation.operator.clear(),
        |p| p.operator_attestation.approval_record_id = Uuid::nil(),
        |p| p.operator_attestation.reviewed_at.clear(),
        |p| p.operator_attestation.source_evidence.clear(),
        |p| p.operator_attestation.destination_evidence.clear(),
        |p| p.operator_attestation.content_evidence.clear(),
        |p| p.operator_attestation.decision_sha256 = "c".repeat(64),
        |p| p.binding.destination_area = AreaId::Trust,
        |p| p.decision_id = id(999),
    ] {
        let mut changed = manifest.clone();
        mutate(&mut changed.provenance_projections[0]);
        assert_blocked(&inventory, &changed, "operator attestation");
    }
}

#[test]
fn attested_binding_mismatch_unknown_edge_and_wrong_historical_version_are_rejected() {
    let (inventory, manifest) = projection_fixture();
    for mutate in [
        (|p: &mut ProvenanceProjection| p.binding.source_instance = id(99))
            as fn(&mut ProvenanceProjection),
        |p| p.binding.snapshot_id = id(99),
        |p| p.binding.inventory_sha256 = "c".repeat(64),
        |p| p.binding.source.row_sha256 = "c".repeat(64),
        |p| p.binding.archived_task.row_sha256 = "c".repeat(64),
        |p| p.binding.memory_version.row_sha256 = "c".repeat(64),
        |p| p.binding.destination_area = AreaId::Trust,
        |p| p.binding.canonical_id = id(999),
        |p| p.binding.memory_version.record.key = id(10).to_string(),
        |p| {
            p.binding.memory_version.record.key = id(10).to_string();
            p.binding.version.version_no = 1;
        },
        |p| p.binding.version.version_no = 1,
        |p| p.binding.version.is_tombstone = true,
        |p| p.binding.source.record.key = id(999).to_string(),
        |p| p.binding.task_constraint = "logical:target_memory_id".into(),
        |p| p.binding.archived_task.record.table = "public.raw_captures".into(),
    ] {
        let mut changed = manifest.clone();
        mutate(&mut changed.provenance_projections[0]);
        attest(&mut changed.provenance_projections[0]);
        assert_blocked(&inventory, &changed, "projection");
    }
}

#[test]
fn duplicate_source_decisions_and_replayed_approval_records_are_rejected() {
    let (inventory, mut manifest) = projection_fixture();
    manifest
        .provenance_projections
        .push(manifest.provenance_projections[0].clone());
    assert_blocked(
        &inventory,
        &manifest,
        "unsupported/duplicate provenance projection",
    );
    manifest.provenance_projections[1].decision_id = id(99);
    attest(&mut manifest.provenance_projections[1]);
    assert_blocked(&inventory, &manifest, "operator attestation");
}

#[test]
fn reviewed_excerpt_is_exact_bounded_content_and_never_leaks_in_blockers() {
    use sha2::{Digest, Sha256};
    let (inventory, mut manifest) = projection_fixture();
    let text = "Only this synthetic fact, not the unrelated original task context.".to_owned();
    manifest.provenance_projections[0].content = ProjectedSourceContent::ReviewedExcerpt {
        text: text.clone(),
        text_sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
    };
    attest(&mut manifest.provenance_projections[0]);
    assert!(verify_manifest(&inventory, &manifest).plan_valid);
    for bad in [
        String::new(),
        "PRIVATE-TEST-SENTINEL".into(),
        "é".repeat(MAX_REVIEWED_EXCERPT_BYTES / 2 + 1),
        "bad\0excerpt".into(),
    ] {
        let mut changed = manifest.clone();
        let text_sha256 = if bad == "PRIVATE-TEST-SENTINEL" {
            "c".repeat(64)
        } else {
            format!("{:x}", Sha256::digest(bad.as_bytes()))
        };
        changed.provenance_projections[0].content = ProjectedSourceContent::ReviewedExcerpt {
            text: bad,
            text_sha256,
        };
        attest(&mut changed.provenance_projections[0]);
        assert_blocked(&inventory, &changed, "excerpt bytes/digest");
        assert!(
            !verify_manifest(&inventory, &changed)
                .blockers
                .join(" ")
                .contains("PRIVATE-TEST-SENTINEL")
        );
    }
}

#[test]
fn null_task_and_unknown_source_check_edges_do_not_gain_a_projection_exception() {
    let (mut inventory, mut manifest) = projection_fixture();
    table_mut(&mut inventory, "public.memory_sources")
        .columns
        .iter_mut()
        .find(|column| column.name == "task_id")
        .expect("task column")
        .nullable = true;
    table_mut(&mut inventory, "public.memory_sources").rows[0]
        .references
        .iter_mut()
        .find(|reference| reference.constraint == "source_task")
        .expect("task reference")
        .target = None;
    assert!(verify_inventory(&inventory).is_empty());
    manifest.inventory_sha256 = inventory_digest(&inventory).expect("inventory digest");
    manifest.provenance_projections[0].binding.inventory_sha256 = manifest.inventory_sha256.clone();
    attest(&mut manifest.provenance_projections[0]);
    assert_blocked(&inventory, &manifest, "declared edge mismatch");

    let (mut inventory, mut manifest) = projection_fixture();
    let checks = table_mut(&mut inventory, "public.memory_source_checks");
    checks
        .columns
        .iter_mut()
        .find(|column| column.name == "source_id")
        .expect("source column")
        .name = "unreviewed_source_link".into();
    checks.foreign_keys[0].columns = vec!["unreviewed_source_link".into()];
    assert!(verify_inventory(&inventory).is_empty());
    manifest.inventory_sha256 = inventory_digest(&inventory).expect("inventory digest");
    manifest.provenance_projections[0].binding.inventory_sha256 = manifest.inventory_sha256.clone();
    attest(&mut manifest.provenance_projections[0]);
    assert_blocked(&inventory, &manifest, "unsupported dependent source-check");
}

#[test]
fn cli_validates_v2_projection_and_still_reports_no_import_support() {
    let (inventory, manifest) = projection_fixture();
    let temp = std::env::temp_dir().join(format!("mem-area-v2-test-{}", Uuid::new_v4()));
    std::fs::create_dir(&temp).expect("temporary synthetic fixture directory");
    let inventory_path = temp.join("inventory.json");
    let manifest_path = temp.join("manifest.json");
    std::fs::write(
        &inventory_path,
        serde_json::to_vec(&inventory).expect("inventory JSON"),
    )
    .expect("write synthetic inventory");
    std::fs::write(
        &manifest_path,
        serde_json::to_vec(&manifest).expect("manifest JSON"),
    )
    .expect("write synthetic manifest");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_memory-area"))
        .args(["verify", "--inventory"])
        .arg(&inventory_path)
        .arg("--manifest")
        .arg(&manifest_path)
        .output()
        .expect("run offline CLI");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Verification = serde_json::from_slice(&output.stdout).expect("verification JSON");
    assert!(report.plan_valid);
    assert!(!report.import_supported);
    assert_eq!(
        report.provenance_projections,
        manifest.provenance_projections
    );
    std::fs::remove_file(inventory_path).expect("remove synthetic inventory");
    std::fs::remove_file(manifest_path).expect("remove synthetic manifest");
    std::fs::remove_dir(temp).expect("remove empty synthetic fixture directory");
}

#[test]
fn projection_cannot_copy_task_session_or_bypass_another_cross_area_reference() {
    let (inventory, manifest) = projection_fixture();
    for key in [20, 21] {
        let mut changed = manifest.clone();
        changed
            .records
            .iter_mut()
            .find(|decision| decision.record.key == id(key).to_string())
            .expect("synthetic fixture")
            .disposition = Disposition::Area {
            area: AreaId::General,
            review: review(SourceClassification::Knowledge),
        };
        assert_blocked(&inventory, &changed, "must remain protected archive");
    }
    let mut inventory = inventory;
    push(
        &mut inventory,
        "public.memory_entries",
        row(
            id(40).to_string(),
            Some(id(1)),
            Some(id(40)),
            Some(VersionIdentity {
                version_no: 1,
                is_tombstone: false,
            }),
        ),
    );
    push(
        &mut inventory,
        "public.memory_canonical_state",
        row(id(40).to_string(), Some(id(1)), Some(id(40)), None),
    );
    table_mut(&mut inventory, "public.memory_sources").rows[0]
        .references
        .iter_mut()
        .find(|reference| reference.constraint == "logical:target_memory_id")
        .expect("synthetic fixture")
        .target = Some(RecordRef {
        table: "public.memory_entries".into(),
        key: id(40).to_string(),
    });
    let mut changed = manifest;
    changed.inventory_sha256 = inventory_digest(&inventory).expect("synthetic fixture");
    changed.lineages.push(LineageDecision {
        canonical_id: id(40),
        source_project_ids: vec![id(1)],
        disposition: Disposition::ProtectedArchive {
            reason: "Other archived lineage".into(),
            review: review(SourceClassification::Ambiguous),
        },
    });
    changed.provenance_projections[0].binding.inventory_sha256 = changed.inventory_sha256.clone();
    attest(&mut changed.provenance_projections[0]);
    assert_blocked(&inventory, &changed, "requires reviewed transformation");
}

#[test]
fn complete_review_preserves_every_version_tombstone_hash_and_project() {
    let inventory = fixture();
    assert_eq!(verify_inventory(&inventory), Vec::<String>::new());
    let report = verify_manifest(&inventory, &reviewed(&inventory));
    assert!(report.plan_valid, "{:?}", report.blockers);
    assert!(!report.import_supported);
    assert_eq!(report.records.len(), 5);
    let versions: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.record.table == "public.memory_entries")
        .collect();
    assert_eq!(versions.len(), 3);
    for version in versions {
        assert_eq!(version.destination, Destination::Area(AreaId::General));
        assert_eq!(version.source_project_id, Some(id(1)));
        assert_eq!(version.canonical_id, Some(id(10)));
        assert_eq!(version.source_sha256, "a".repeat(64));
    }
}

#[test]
fn draft_does_not_infer_certificate_authority_from_ca_project_name() {
    let inventory = fixture();
    let manifest = draft_manifest(&inventory).expect("fixture");
    assert_eq!(manifest.lineages[0].disposition, Disposition::Unresolved);
    assert!(manifest.projects.is_empty());
    assert_blocked(&inventory, &manifest, "unresolved ownership");
}

#[test]
fn duplicate_lineage_owners_and_version_overrides_are_rejected() {
    let inventory = fixture();
    let mut manifest = reviewed(&inventory);
    let mut competing = manifest.lineages[0].clone();
    competing.disposition = Disposition::Area {
        area: AreaId::Trust,
        review: review(SourceClassification::Knowledge),
    };
    manifest.lineages.push(competing);
    assert_blocked(&inventory, &manifest, "duplicate canonical ownership");
    manifest.lineages.pop();
    manifest.records.push(RecordDecision {
        record: RecordRef {
            table: "public.memory_entries".into(),
            key: id(12).to_string(),
        },
        disposition: Disposition::Area {
            area: AreaId::Trust,
            review: review(SourceClassification::Knowledge),
        },
    });
    assert_blocked(
        &inventory,
        &manifest,
        "cannot override lineage/runtime policy",
    );
}

#[test]
fn protected_source_classes_cannot_become_retrievable_knowledge() {
    for class in [
        SourceClassification::Test,
        SourceClassification::Evaluation,
        SourceClassification::Ambiguous,
        SourceClassification::Mixed,
    ] {
        let inventory = fixture();
        let mut manifest = reviewed(&inventory);
        manifest.lineages[0].disposition = Disposition::Area {
            area: AreaId::General,
            review: review(class),
        };
        assert_blocked(&inventory, &manifest, "requires protected archive");
        manifest.lineages[0].disposition = Disposition::ProtectedArchive {
            reason: "operator-only non-retrievable archive".into(),
            review: review(class),
        };
        manifest.projects.clear();
        let report = verify_manifest(&inventory, &manifest);
        assert!(report.plan_valid, "{:?}", report.blockers);
        assert!(
            report
                .records
                .iter()
                .all(|row| row.destination == Destination::ProtectedArchive)
        );
    }
}

#[test]
fn source_row_hash_schema_and_project_changes_invalidate_review() {
    let inventory = fixture();
    let manifest = reviewed(&inventory);
    let mut changed = inventory.clone();
    table_mut(&mut changed, "public.memory_entries").rows[0].row_sha256 = "b".repeat(64);
    assert_blocked(&changed, &manifest, "inventory digest mismatch");
    let mut changed = inventory.clone();
    table_mut(&mut changed, "public.memory_entries")
        .columns
        .push(Column {
            name: "new_private_data".into(),
            data_type: "text".into(),
            nullable: true,
        });
    assert_blocked(&changed, &manifest, "inventory digest mismatch");
    let mut changed = inventory.clone();
    changed.projects[0].root_path = "/changed/source".into();
    assert_blocked(
        &changed,
        &manifest,
        "changed or unjustified project provenance",
    );
}

#[test]
fn incomplete_counts_keys_versions_tables_and_hashes_block_inventory() {
    let mut inventory = fixture();
    table_mut(&mut inventory, "public.memory_entries").row_count += 1;
    assert!(
        verify_inventory(&inventory)
            .iter()
            .any(|error| error.contains("row count mismatch"))
    );
    let mut inventory = fixture();
    let duplicate = table_mut(&mut inventory, "public.memory_entries").rows[0].clone();
    push(&mut inventory, "public.memory_entries", duplicate);
    let errors = verify_inventory(&inventory);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("duplicate row key"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("duplicate canonical version"))
    );
    let mut inventory = fixture();
    inventory
        .tables
        .retain(|table| table.table != "public.auth_service_tokens");
    assert!(
        verify_inventory(&inventory)
            .iter()
            .any(|error| error.contains("missing table inventory public.auth_service_tokens"))
    );
    let mut inventory = fixture();
    table_mut(&mut inventory, "public.memory_entries").rows[0].row_sha256 = "bad".into();
    assert!(
        verify_inventory(&inventory)
            .iter()
            .any(|error| error.contains("invalid complete-row SHA-256"))
    );
}

#[test]
fn unknown_tables_cannot_be_manually_whitelisted() {
    let mut inventory = fixture();
    let mut table = table_mut(&mut inventory, "public.auth_service_tokens").clone();
    table.table = "public.auth_new_secrets".into();
    inventory.tables.push(table);
    let mut manifest = reviewed(&inventory);
    let policy = manifest
        .table_policies
        .iter_mut()
        .find(|policy| policy.table == "public.auth_new_secrets")
        .expect("unknown table draft policy");
    assert!(policy.strategy.is_none());
    policy.strategy = Some(TableStrategy::HistoricalArchive);
    assert_blocked(
        &inventory,
        &manifest,
        "unclassified table public.auth_new_secrets",
    );
}

const HISTORICAL_SCHEMA: &str = "mab_full_20260516_150014";
const HISTORICAL_NAMES: &[&str] = &[
    "_sqlx_migrations",
    "curation_runs",
    "memory_chunks",
    "memory_entries",
    "memory_relations",
    "memory_sources",
    "memory_tags",
    "projects",
    "raw_captures",
    "sessions",
    "tasks",
];

fn historical_fixture() -> Inventory {
    let mut inventory = fixture();
    for name in HISTORICAL_NAMES {
        let key = if *name == "projects" {
            id(501)
        } else {
            id(502)
        };
        inventory.tables.push(TableInventory {
            table: format!("{HISTORICAL_SCHEMA}.{name}"),
            columns: vec![Column {
                name: "id".into(),
                data_type: "uuid".into(),
                nullable: false,
            }],
            foreign_keys: vec![],
            row_count: 1,
            rows: vec![row(
                key.to_string(),
                (*name != "projects").then_some(id(501)),
                None,
                None,
            )],
        });
    }
    inventory
}

#[test]
fn exact_historical_catalog_is_optional_and_archives_every_row() {
    assert!(
        verify_inventory(&fixture()).is_empty(),
        "historical schema is optional"
    );
    let inventory = historical_fixture();
    assert_eq!(verify_inventory(&inventory), Vec::<String>::new());
    let manifest = reviewed(&inventory);
    assert_eq!(
        manifest.lineages.len(),
        1,
        "historical rows create no public lineage decisions"
    );
    assert!(
        manifest.records.is_empty(),
        "historical tables need no area ownership reviews"
    );
    let report = verify_manifest(&inventory, &manifest);
    assert!(report.plan_valid, "{:?}", report.blockers);
    let archived: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.record.table.starts_with(HISTORICAL_SCHEMA))
        .collect();
    assert_eq!(archived.len(), 11);
    assert!(
        archived
            .iter()
            .all(|record| record.destination == Destination::ProtectedArchive)
    );
    for name in HISTORICAL_NAMES {
        assert_eq!(
            table_strategy(&format!("{HISTORICAL_SCHEMA}.{name}")),
            Some(TableStrategy::HistoricalArchive)
        );
    }
}

#[test]
fn partial_historical_catalog_and_new_tables_or_schemas_stay_blocked() {
    let mut inventory = historical_fixture();
    inventory
        .tables
        .retain(|table| table.table != format!("{HISTORICAL_SCHEMA}.tasks"));
    assert_blocked(
        &inventory,
        &reviewed(&inventory),
        "missing historical table inventory mab_full_20260516_150014.tasks",
    );
    for unknown in [
        "mab_full_20260516_150014.new_table",
        "mab_full_20260516_150015.memory_entries",
        "mab_full_20260516_150014_extra.projects",
    ] {
        let mut inventory = historical_fixture();
        let mut table = inventory.tables.last().expect("table").clone();
        table.table = unknown.into();
        inventory.tables.push(table);
        assert_eq!(table_strategy(unknown), None);
        assert_blocked(
            &inventory,
            &reviewed(&inventory),
            &format!("unclassified table {unknown}"),
        );
    }
}

#[test]
fn historical_project_provenance_is_schema_local() {
    let mut inventory = historical_fixture();
    assert!(
        !inventory
            .projects
            .iter()
            .any(|project| project.id == id(501))
    );
    assert!(
        verify_inventory(&inventory).is_empty(),
        "historical-only UUID is valid locally"
    );
    table_mut(
        &mut inventory,
        &format!("{HISTORICAL_SCHEMA}.memory_entries"),
    )
    .rows[0]
        .source_project_id = Some(id(1));
    let validation = inspect_inventory(&inventory);
    assert!(validation.blockers.is_empty());
    assert_eq!(validation.notices.len(), 1);
    assert_eq!(validation.notices[0].source_project_id, id(1));
    assert_eq!(
        validation.notices[0].project_table,
        format!("{HISTORICAL_SCHEMA}.projects")
    );
    // A public UUID cannot substitute for an absent historical source project;
    // its original unresolved local reference remains visible in the archive.
    assert!(inventory.projects.iter().any(|project| project.id == id(1)));
    let mut invalid = historical_fixture();
    table_mut(&mut invalid, &format!("{HISTORICAL_SCHEMA}.projects")).rows[0].key =
        "not-a-uuid".into();
    assert_blocked(
        &invalid,
        &reviewed(&invalid),
        "invalid historical project identity",
    );
}

#[test]
fn historical_foreign_keys_remain_accounted_and_cannot_be_promoted() {
    let mut inventory = historical_fixture();
    let entries = table_mut(
        &mut inventory,
        &format!("{HISTORICAL_SCHEMA}.memory_entries"),
    );
    entries.columns.push(Column {
        name: "project_id".into(),
        data_type: "uuid".into(),
        nullable: false,
    });
    entries.foreign_keys.push(ForeignKey {
        name: "historical_project_fk".into(),
        columns: vec!["project_id".into()],
        target_table: format!("{HISTORICAL_SCHEMA}.projects"),
        target_columns: vec!["id".into()],
    });
    entries.rows[0].references.push(RowReference {
        constraint: "historical_project_fk".into(),
        target: Some(RecordRef {
            table: format!("{HISTORICAL_SCHEMA}.projects"),
            key: id(501).to_string(),
        }),
    });
    let manifest = reviewed(&inventory);
    assert!(verify_manifest(&inventory, &manifest).plan_valid);
    let mut changed = manifest.clone();
    changed
        .table_policies
        .iter_mut()
        .find(|policy| policy.table == format!("{HISTORICAL_SCHEMA}.memory_entries"))
        .expect("historical policy")
        .strategy = Some(TableStrategy::Owned);
    assert_blocked(&inventory, &changed, "invalid or unclassified table policy");
    let mut overridden = manifest.clone();
    overridden.records.push(RecordDecision {
        record: RecordRef {
            table: format!("{HISTORICAL_SCHEMA}.memory_entries"),
            key: id(502).to_string(),
        },
        disposition: Disposition::Area {
            area: AreaId::General,
            review: review(SourceClassification::Knowledge),
        },
    });
    assert_blocked(&inventory, &overridden, "extraneous record decision");
    table_mut(
        &mut inventory,
        &format!("{HISTORICAL_SCHEMA}.memory_entries"),
    )
    .rows[0]
        .references[0]
        .target
        .as_mut()
        .expect("target")
        .key = id(999).to_string();
    assert_blocked(&inventory, &reviewed(&inventory), "dangling reference");
}

#[test]
fn streamed_inventory_digest_preserves_format_one_bytes() {
    use sha2::{Digest, Sha256};
    let mut inventory = historical_fixture();
    inventory.projects[0].name = "Name with unicode £ and \"quoted\" text\n".into();
    inventory.tables.reverse();
    let entries = table_mut(&mut inventory, "public.memory_entries");
    entries.rows.reverse();
    entries.columns.push(Column {
        name: "a_first".into(),
        data_type: "text".into(),
        nullable: true,
    });
    entries.foreign_keys.push(ForeignKey {
        name: "z_last".into(),
        columns: vec!["id".into(), "a_first".into()],
        target_table: "public.projects".into(),
        target_columns: vec!["id".into(), "slug".into()],
    });
    entries.foreign_keys.push(ForeignKey {
        name: "a_first".into(),
        columns: vec!["id".into()],
        target_table: "public.projects".into(),
        target_columns: vec!["id".into()],
    });
    entries.rows[0].references = vec![
        RowReference {
            constraint: "z_last".into(),
            target: None,
        },
        RowReference {
            constraint: "a_first".into(),
            target: Some(RecordRef {
                table: "public.projects".into(),
                key: id(1).to_string(),
            }),
        },
    ];
    // Reference implementation is the original format-v1 normalization. Keep
    // this comparison so reducing allocations never invalidates reviewed hashes.
    let mut normalized = inventory.clone();
    normalized.projects.sort_by_key(|project| project.id);
    normalized.tables.sort_by(|a, b| a.table.cmp(&b.table));
    for table in &mut normalized.tables {
        table.columns.sort_by(|a, b| a.name.cmp(&b.name));
        table.foreign_keys.sort_by(|a, b| a.name.cmp(&b.name));
        table.rows.sort_by(|a, b| a.key.cmp(&b.key));
        for row in &mut table.rows {
            row.references
                .sort_by(|a, b| a.constraint.cmp(&b.constraint));
        }
    }
    let legacy = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&normalized).expect("legacy serialization"))
    );
    assert_eq!(
        inventory_digest(&inventory).expect("streamed digest"),
        legacy
    );
}

#[test]
fn tokens_jobs_and_federation_authority_stay_retired() {
    let mut inventory = fixture();
    for table in [
        "public.auth_service_tokens",
        "public.background_jobs",
        "public.federation_local_grants",
    ] {
        push(
            &mut inventory,
            table,
            row("synthetic-runtime".into(), None, None, None),
        );
    }
    let mut manifest = reviewed(&inventory);
    let report = verify_manifest(&inventory, &manifest);
    assert!(report.plan_valid, "{:?}", report.blockers);
    assert_eq!(
        report
            .records
            .iter()
            .filter(|row| row.destination == Destination::ArchiveAndRetire)
            .count(),
        3
    );
    manifest
        .table_policies
        .iter_mut()
        .find(|policy| policy.table == "public.background_jobs")
        .expect("policy")
        .strategy = Some(TableStrategy::Owned);
    assert_blocked(
        &inventory,
        &manifest,
        "invalid or unclassified table policy public.background_jobs",
    );
}

#[test]
fn cross_area_and_mixed_session_references_block_without_transformation() {
    let mut inventory = fixture();
    push(
        &mut inventory,
        "public.sessions",
        row(id(30).to_string(), Some(id(1)), None, None),
    );
    let mut task = row(id(31).to_string(), Some(id(1)), None, None);
    task.references.push(RowReference {
        constraint: "logical:session_id".into(),
        target: Some(RecordRef {
            table: "public.sessions".into(),
            key: id(30).to_string(),
        }),
    });
    push(&mut inventory, "public.tasks", task);
    let mut manifest = reviewed(&inventory);
    for decision in &mut manifest.records {
        decision.disposition = if decision.record.table == "public.sessions" {
            Disposition::ProtectedArchive {
                reason: "contains unrelated private material".into(),
                review: review(SourceClassification::Mixed),
            }
        } else {
            Disposition::Area {
                area: AreaId::General,
                review: review(SourceClassification::Knowledge),
            }
        };
    }
    assert_blocked(&inventory, &manifest, "cross-area/archive reference");
    for decision in &mut manifest.records {
        decision.disposition = Disposition::ProtectedArchive {
            reason: "retain mixed session and dependent task together".into(),
            review: review(SourceClassification::Mixed),
        };
    }
    assert!(verify_manifest(&inventory, &manifest).plan_valid);
}

#[test]
fn foreign_key_catalog_requires_every_row_reference_and_checks_targets() {
    let mut inventory = fixture();
    let entries = table_mut(&mut inventory, "public.memory_entries");
    entries.columns.push(Column {
        name: "project_id".into(),
        data_type: "uuid".into(),
        nullable: false,
    });
    entries.foreign_keys.push(ForeignKey {
        name: "memory_entries_project_id_fkey".into(),
        columns: vec!["project_id".into()],
        target_table: "public.projects".into(),
        target_columns: vec!["id".into()],
    });
    assert!(
        verify_inventory(&inventory)
            .iter()
            .any(|error| error.contains("missing FK accounting"))
    );
    for row in &mut table_mut(&mut inventory, "public.memory_entries").rows {
        row.references.push(RowReference {
            constraint: "memory_entries_project_id_fkey".into(),
            target: Some(RecordRef {
                table: "public.projects".into(),
                key: id(1).to_string(),
            }),
        });
    }
    assert!(verify_inventory(&inventory).is_empty());
    assert!(verify_manifest(&inventory, &reviewed(&inventory)).plan_valid);
    table_mut(&mut inventory, "public.memory_entries").rows[0].references[0].target = None;
    assert!(
        verify_inventory(&inventory)
            .iter()
            .any(|error| error.contains("null non-nullable FK"))
    );
    table_mut(&mut inventory, "public.memory_entries").rows[0].references[0].target =
        Some(RecordRef {
            table: "public.projects".into(),
            key: id(999).to_string(),
        });
    assert!(
        verify_inventory(&inventory)
            .iter()
            .any(|error| error.contains("dangling reference"))
    );
}

#[test]
fn row_order_does_not_change_digest_but_missing_history_does() {
    let mut inventory = fixture();
    let digest = inventory_digest(&inventory).expect("digest");
    inventory.tables.reverse();
    table_mut(&mut inventory, "public.memory_entries")
        .rows
        .reverse();
    assert_eq!(inventory_digest(&inventory).expect("digest"), digest);
    let manifest = reviewed(&inventory);
    let entries = table_mut(&mut inventory, "public.memory_entries");
    entries.rows.pop();
    entries.row_count -= 1;
    assert_blocked(&inventory, &manifest, "inventory digest mismatch");
}

#[test]
fn reviewed_schema_catalog_matches_every_current_repository_table() {
    let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let mut paths: Vec<_> = std::fs::read_dir(migrations)
        .expect("migrations directory")
        .map(|entry| entry.expect("migration entry").path())
        .collect();
    paths.sort();
    let mut active = BTreeSet::from(["public._sqlx_migrations".to_owned()]);
    for path in paths {
        if path.extension().is_none_or(|extension| extension != "sql") {
            continue;
        }
        for line in std::fs::read_to_string(path)
            .expect("migration source")
            .lines()
        {
            if let Some(rest) = line.strip_prefix("CREATE TABLE ") {
                let rest = rest.strip_prefix("IF NOT EXISTS ").unwrap_or(rest);
                let name = rest.split_whitespace().next().expect("created table name");
                active.insert(format!("public.{name}"));
            } else if let Some(rest) = line.strip_prefix("DROP TABLE ") {
                let name = rest.trim_end_matches(';');
                active.remove(&format!("public.{name}"));
            }
        }
    }
    let catalog: BTreeSet<_> = KNOWN_TABLES
        .iter()
        .chain(SUPERVISOR_TABLES_0053)
        .chain(SUPERVISOR_TABLES_0054)
        .map(|(name, _)| name.to_string())
        .collect();
    assert_eq!(
        catalog.len(),
        KNOWN_TABLES.len() + SUPERVISOR_TABLES_0053.len() + SUPERVISOR_TABLES_0054.len(),
        "duplicate table policy"
    );
    assert_eq!(
        catalog, active,
        "schema changes require explicit policy review"
    );
}

#[test]
fn migration_0053_is_exact_version_gated_and_cannot_reactivate_supervisor_state() {
    let baseline = fixture();
    assert_eq!(SUPPORTED_MIGRATION, 52);
    assert_eq!(KNOWN_TABLES.len(), 93);
    assert!(verify_inventory(&baseline).is_empty());
    let baseline_digest = inventory_digest(&baseline).expect("baseline digest");
    let baseline_manifest = serde_json::to_vec(&reviewed(&baseline)).expect("baseline manifest");
    assert_eq!(SUPERVISOR_TABLES_0053.len(), 10);
    let mut inventory = baseline.clone();
    for (table, _) in SUPERVISOR_TABLES_0053 {
        inventory.tables.push(TableInventory {
            table: (*table).into(),
            columns: vec![Column {
                name: "id".into(),
                data_type: "uuid".into(),
                nullable: false,
            }],
            foreign_keys: vec![],
            row_count: 1,
            rows: vec![row(id(99).to_string(), None, None, None)],
        });
    }
    let mixed = verify_inventory(&inventory);
    assert_eq!(
        mixed
            .iter()
            .filter(
                |error| error.starts_with("migration 0053 table present at migration 0052 head:")
            )
            .count(),
        10
    );
    inventory.migration_version = SUPERVISOR_MIGRATION;
    assert!(verify_inventory(&inventory).is_empty());
    let manifest = reviewed(&inventory);
    let report = verify_manifest(&inventory, &manifest);
    assert!(report.plan_valid, "{:?}", report.blockers);
    for (table, strategy) in SUPERVISOR_TABLES_0053 {
        let planned = report
            .records
            .iter()
            .find(|record| record.record.table == *table)
            .expect("retained supervisor record");
        assert_eq!(
            planned.destination,
            match strategy {
                TableStrategy::HistoricalArchive => Destination::ProtectedArchive,
                TableStrategy::RetireRuntime => Destination::ArchiveAndRetire,
                _ => panic!("supervisor tables cannot be active knowledge"),
            }
        );
    }
    for (missing, _) in SUPERVISOR_TABLES_0053 {
        let mut incomplete = inventory.clone();
        incomplete.tables.retain(|table| table.table != *missing);
        assert!(
            verify_inventory(&incomplete)
                .iter()
                .any(|error| error == &format!("missing migration 0053 table inventory {missing}"))
        );
    }
    let mut unknown = inventory.clone();
    let mut table = unknown.tables.last().expect("table").clone();
    table.table = "public.supervisor_future_authority".into();
    unknown.tables.push(table);
    assert!(
        verify_inventory(&unknown)
            .iter()
            .any(|error| error.contains("unclassified table public.supervisor_future_authority"))
    );
    for unsupported in [51, 55, 999] {
        let mut future = inventory.clone();
        future.migration_version = unsupported;
        assert!(
            verify_inventory(&future)
                .iter()
                .any(|error| error.starts_with("unsupported migration version"))
        );
    }
    inventory.tables.retain(|table| {
        !SUPERVISOR_TABLES_0053
            .iter()
            .any(|(name, _)| *name == table.table)
    });
    inventory.migration_version = SUPPORTED_MIGRATION;
    assert_eq!(inventory, baseline);
    assert_eq!(
        inventory_digest(&inventory).expect("same digest"),
        baseline_digest
    );
    assert_eq!(
        serde_json::to_vec(&reviewed(&inventory)).expect("same manifest"),
        baseline_manifest
    );
}

#[test]
fn migration_0054_requires_exact_additive_catalog_and_archives_private_proposals() {
    assert_eq!(SUPERVISOR_TABLES_0054.len(), 1);
    assert_eq!(SUPERVISOR_PROPOSAL_MIGRATION, 54);
    let baseline = fixture();
    let mut inventory = baseline.clone();
    for (table, _) in SUPERVISOR_TABLES_0053.iter().chain(SUPERVISOR_TABLES_0054) {
        inventory.tables.push(TableInventory {
            table: (*table).into(),
            columns: vec![Column {
                name: "id".into(),
                data_type: "uuid".into(),
                nullable: false,
            }],
            foreign_keys: vec![],
            row_count: 1,
            rows: vec![row(id(990).to_string(), None, None, None)],
        });
    }
    for old_head in [SUPPORTED_MIGRATION, SUPERVISOR_MIGRATION] {
        inventory.migration_version = old_head;
        assert!(
            verify_inventory(&inventory)
                .iter()
                .any(|error| error.contains("migration 0054 table present before"))
        );
    }
    inventory.migration_version = SUPERVISOR_PROPOSAL_MIGRATION;
    assert!(verify_inventory(&inventory).is_empty());
    let manifest = reviewed(&inventory);
    let report = verify_manifest(&inventory, &manifest);
    assert!(report.plan_valid, "{:?}", report.blockers);
    assert_eq!(
        report
            .records
            .iter()
            .find(|record| record.record.table == "public.supervisor_proposals")
            .expect("synthetic fixture")
            .destination,
        Destination::ProtectedArchive
    );
    for (name, _) in SUPERVISOR_TABLES_0053.iter().chain(SUPERVISOR_TABLES_0054) {
        let mut missing = inventory.clone();
        missing.tables.retain(|table| table.table != *name);
        assert!(
            verify_inventory(&missing)
                .iter()
                .any(|error| error.starts_with("missing migration"))
        );
    }
    let mut forbidden = manifest.clone();
    forbidden
        .table_policies
        .iter_mut()
        .find(|policy| policy.table == "public.supervisor_proposals")
        .expect("synthetic fixture")
        .strategy = Some(TableStrategy::Owned);
    assert_blocked(
        &inventory,
        &forbidden,
        "invalid or unclassified table policy",
    );
    forbidden = manifest;
    forbidden.records.push(RecordDecision {
        record: RecordRef {
            table: "public.supervisor_proposals".into(),
            key: id(990).to_string(),
        },
        disposition: Disposition::Area {
            area: AreaId::Workshop,
            review: review(SourceClassification::Knowledge),
        },
    });
    assert_blocked(
        &inventory,
        &forbidden,
        "cannot override lineage/runtime policy",
    );
    let mut unknown = inventory.clone();
    let mut extra = unknown.tables.last().expect("synthetic fixture").clone();
    extra.table = "public.supervisor_proposals_future".into();
    unknown.tables.push(extra);
    assert!(
        verify_inventory(&unknown)
            .iter()
            .any(|error| error.contains("unclassified table public.supervisor_proposals_future"))
    );
    assert!(verify_inventory(&baseline).is_empty());
}

#[test]
fn serde_rejects_unknown_fields_and_stable_area_ids_round_trip() {
    for (area, wire) in [
        (AreaId::Trust, "trust"),
        (AreaId::Homelab, "homelab"),
        (AreaId::Workshop, "workshop"),
        (AreaId::General, "general"),
    ] {
        assert_eq!(
            serde_json::to_string(&area).expect("serialize"),
            format!("\"{wire}\"")
        );
    }
    let mut value = serde_json::to_value(fixture()).expect("fixture JSON");
    value["ignored_private_table"] = serde_json::json!({});
    assert!(serde_json::from_value::<Inventory>(value).is_err());
}

#[test]
fn cli_only_emits_reviewable_offline_plans_and_reports_blockers_with_exit_two() {
    let temp = std::env::temp_dir().join(format!("mem-area-test-{}", Uuid::new_v4()));
    std::fs::create_dir(&temp).expect("temporary fixture directory");
    let inventory = fixture();
    let inventory_path = temp.join("inventory.json");
    let manifest_path = temp.join("manifest.json");
    std::fs::write(
        &inventory_path,
        serde_json::to_vec(&inventory).expect("inventory JSON"),
    )
    .expect("write synthetic fixture");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_memory-area"))
        .args(["inventory", "--input"])
        .arg(&inventory_path)
        .output()
        .expect("run inventory");
    assert!(output.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("inventory output");
    assert_eq!(report["inventory_valid"], true);
    assert_eq!(report["import_supported"], false);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_memory-area"))
        .args(["plan", "--inventory"])
        .arg(&inventory_path)
        .output()
        .expect("run plan");
    assert!(output.status.success());
    let draft: Manifest = serde_json::from_slice(&output.stdout).expect("manifest output");
    assert_eq!(draft.lineages[0].disposition, Disposition::Unresolved);
    std::fs::write(&manifest_path, &output.stdout).expect("write draft");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_memory-area"))
        .args(["verify", "--inventory"])
        .arg(&inventory_path)
        .arg("--manifest")
        .arg(&manifest_path)
        .output()
        .expect("run verify");
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("verification output");
    assert_eq!(report["plan_valid"], false);
    assert_eq!(report["import_supported"], false);
    assert_eq!(report["records"], serde_json::json!([]));
    std::fs::remove_dir_all(&temp).expect("remove only synthetic fixture directory");
}
