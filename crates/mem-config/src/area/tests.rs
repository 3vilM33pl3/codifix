// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

struct Fixture {
    root: PathBuf,
    routing: AreaRouting,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("memory-area-routing-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let mut areas = BTreeMap::new();
        for name in ["general", "workshop", "homelab"] {
            let instance_id = Uuid::new_v4();
            let config = root.join(format!("{name}.toml"));
            std::fs::write(&config, format!("[area]\nname = '{name}'\ninstance_id = '{instance_id}'\n[service]\n[database]\nurl = 'postgresql://example.invalid/test'\n")).unwrap();
            areas.insert(
                name.into(),
                AreaTarget {
                    instance_id,
                    config: Some(config),
                },
            );
        }
        Self {
            root,
            routing: AreaRouting {
                version: 1,
                default_area: "general".into(),
                areas,
                routes: vec![],
            },
        }
    }
    fn route(&mut self, area: &str, repo: Option<&str>, project: Option<&str>) {
        self.routing.routes.push(AreaRoute {
            area: area.into(),
            repository: repo.map(|p| self.root.join(p)),
            project: project.map(str::to_owned),
        });
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn exact_repository_resolves_same_project_names_and_explicit_area_wins() {
    let mut fixture = Fixture::new();
    fixture.route("homelab", Some("infra"), Some("shared-name"));
    fixture.route("workshop", Some("code"), Some("shared-name"));
    fixture.route("homelab", None, Some("shared-name"));
    fixture.route("workshop", None, Some("shared-name"));
    assert!(
        fixture
            .routing
            .resolve(None, None, Some("shared-name"))
            .is_err()
    );
    assert_eq!(
        fixture
            .routing
            .resolve(None, Some(&fixture.root.join("infra")), Some("shared-name"))
            .unwrap()
            .identity
            .name,
        "homelab"
    );
    assert_eq!(
        fixture
            .routing
            .resolve(
                Some("workshop"),
                Some(&fixture.root.join("infra")),
                Some("shared-name")
            )
            .unwrap()
            .identity
            .name,
        "workshop"
    );
    assert_eq!(
        fixture
            .routing
            .resolve(None, None, Some("unmapped"))
            .unwrap()
            .identity
            .name,
        "general"
    );
}

#[test]
fn matching_missing_or_remote_only_area_never_falls_back() {
    let mut fixture = Fixture::new();
    fixture.route("workshop", None, Some("memory"));
    let config = fixture.routing.areas["workshop"].config.as_ref().unwrap();
    std::fs::remove_file(config).unwrap();
    assert!(
        fixture
            .routing
            .resolve(None, None, Some("memory"))
            .unwrap_err()
            .to_string()
            .contains("no fallback")
    );
    fixture.routing.areas.get_mut("workshop").unwrap().config = None;
    assert!(
        fixture
            .routing
            .resolve(None, None, Some("memory"))
            .unwrap_err()
            .to_string()
            .contains("remote-only")
    );
    assert!(
        fixture
            .routing
            .resolve(Some("unknown"), None, None)
            .is_err()
    );
}

#[test]
fn conflicting_exact_routes_fail_closed() {
    let mut fixture = Fixture::new();
    fixture.route("workshop", Some("repo"), None);
    fixture.route("homelab", Some("repo"), None);
    assert!(
        fixture
            .routing
            .resolve(None, Some(&fixture.root.join("repo")), None)
            .is_err()
    );
}

#[test]
fn linked_worktree_uses_the_same_exact_repository_identity() {
    let mut fixture = Fixture::new();
    let repo = fixture.root.join("repo");
    let linked = fixture.root.join("linked");
    let gitdir = repo.join(".git/worktrees/linked");
    std::fs::create_dir_all(&gitdir).unwrap();
    std::fs::create_dir(&linked).unwrap();
    std::fs::write(
        linked.join(".git"),
        format!("gitdir: {}\n", gitdir.display()),
    )
    .unwrap();
    fixture.route("workshop", Some("repo"), None);
    assert_eq!(
        fixture
            .routing
            .resolve(None, Some(&linked), None)
            .unwrap()
            .identity
            .name,
        "workshop"
    );
}

#[test]
fn loaded_config_identity_and_offline_file_are_bound_to_area() {
    let fixture = Fixture::new();
    let selected = fixture
        .routing
        .resolve(Some("workshop"), None, None)
        .unwrap();
    let config = selected.load(Profile::Prod).unwrap();
    let path = config.offline.path.unwrap();
    assert!(path.to_string_lossy().contains(&selected.identity.key()));
    assert_eq!(selected.identity.offline_path(&path), path);
    let general = fixture
        .routing
        .resolve(Some("general"), None, None)
        .unwrap();
    assert_ne!(
        general.load(Profile::Prod).unwrap().offline.path.unwrap(),
        path
    );
    let swapped = ResolvedArea {
        identity: selected.identity,
        config: general.config,
    };
    assert!(
        swapped
            .load(Profile::Prod)
            .unwrap_err()
            .to_string()
            .contains("identity does not match")
    );
}

#[test]
fn duplicate_identity_or_config_is_rejected() {
    let mut fixture = Fixture::new();
    let general = fixture.routing.areas["general"].clone();
    fixture
        .routing
        .areas
        .get_mut("workshop")
        .unwrap()
        .instance_id = general.instance_id;
    assert!(fixture.routing.validate().is_err());
    fixture
        .routing
        .areas
        .get_mut("workshop")
        .unwrap()
        .instance_id = Uuid::new_v4();
    fixture.routing.areas.get_mut("workshop").unwrap().config = general.config;
    assert!(fixture.routing.validate().is_err());
}

#[test]
fn malformed_routing_and_unsafe_permissions_are_errors() {
    let fixture = Fixture::new();
    let path = fixture.root.join("routes.json");
    std::fs::write(&path, serde_json::to_vec(&fixture.routing).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert!(AreaRouting::read(&path).is_ok());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        assert!(
            AreaRouting::read(&path)
                .unwrap_err()
                .to_string()
                .contains("writable")
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    std::fs::write(&path, b"{ malformed").unwrap();
    assert!(AreaRouting::read(&path).is_err());
}

#[test]
fn explicit_offline_paths_remain_distinct_and_stable() {
    let a = AreaIdentityConfig {
        name: "workshop".into(),
        instance_id: Uuid::new_v4(),
    };
    let b = AreaIdentityConfig {
        name: "homelab".into(),
        instance_id: Uuid::new_v4(),
    };
    let old = Path::new("/state/shared.duckdb");
    assert_ne!(a.offline_path(old), b.offline_path(old));
    assert_ne!(a.offline_path(old), old);
    assert_eq!(a.offline_path(&a.offline_path(old)), a.offline_path(old));
}
