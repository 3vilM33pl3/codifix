use super::*;
use codifix_config::agency_observation::AgencyExchangeProject;
use codifix_record::{AuthPrincipalKind, PermissionSet};
use std::collections::BTreeMap;
fn principal(id: Uuid, project: &str) -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        id,
        kind: AuthPrincipalKind::ServiceToken,
        display_name: "test".into(),
        email: None,
        issuer: None,
        subject: None,
        groups: vec![],
        global_role: None,
        global: PermissionSet::EMPTY,
        project_roles: BTreeMap::from([(
            project.into(),
            crate::auth::ProjectRoleGrant {
                role: codifix_record::AuthRole::Operator,
                permissions: PermissionSet::from_permissions(&[Permission::MemoryCurate]),
                source: "test".into(),
            },
        )]),
        credential_source: crate::auth::CredentialSource::ServiceToken,
        token_id: None,
        session_id: None,
        session_csrf_hash: None,
    }
}
#[test]
fn exact_allowlist_and_project_permission_are_both_required() {
    let id = Uuid::new_v4();
    let c = AgencyObservationConfig {
        enabled: true,
        area: "workshop".into(),
        instance_id: Uuid::new_v4(),
        peer_ids: vec!["homelab".into()],
        exchange_projects: vec![AgencyExchangeProject {
            project: "codifix".into(),
            principal_ids: vec![id],
        }],
    };
    let p = principal(id, "codifix");
    assert!(exchange_access(&c, &p, "codifix"));
    assert!(!exchange_access(&c, &p, "other"));
    assert!(!exchange_access(
        &c,
        &principal(Uuid::new_v4(), "codifix"),
        "codifix"
    ));
    assert!(!exchange_access(&c, &principal(id, "other"), "codifix"));
}
#[test]
fn cursor_and_terminal_controls_are_bounded() {
    assert!(cursor(Some("bad")).is_err());
    assert!(cursor(Some(&"x".repeat(129))).is_err());
    let at = Utc::now();
    let id = Uuid::new_v4();
    assert_eq!(
        cursor(Some(&format!("{}|{id}", at.to_rfc3339()))).unwrap(),
        (at, id)
    );
    assert_eq!(safe("a\x1b[31m\rb\n", 6), "a[31mb");
}
