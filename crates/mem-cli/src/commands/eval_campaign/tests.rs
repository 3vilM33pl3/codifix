// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

#[tokio::test]
async fn paid_dispatch_is_rejected_before_reading_files_or_starting_processes() {
    let error = handle(EvalCampaignCommand::Run {
        manifest: "must-not-read".into(),
        driver: "must-not-execute".into(),
        out: "must-not-create".into(),
        split: "held-out".into(),
        allow_shell: true,
        offline: false,
        approvals: vec![],
    })
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("paid campaign execution remains unavailable")
    );
}

#[tokio::test]
async fn offline_driver_requires_explicit_shell_consent() {
    let error = handle(EvalCampaignCommand::Run {
        manifest: "must-not-read".into(),
        driver: "must-not-execute".into(),
        out: "must-not-create".into(),
        split: "development".into(),
        allow_shell: false,
        offline: true,
        approvals: vec![],
    })
    .await
    .unwrap_err();
    assert!(error.to_string().contains("--allow-shell"));
}

#[test]
fn evidence_files_cannot_be_overwritten() {
    let root = std::env::temp_dir().join(format!("memory-r3-evidence-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("receipt.json");
    exclusive_json(&path, &1).unwrap();
    assert!(exclusive_json(&path, &2).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "1");
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(root).unwrap();
    assert!(split("heldout").is_err());
    assert_eq!(split("held-out").unwrap(), Split::HeldOut);
}
