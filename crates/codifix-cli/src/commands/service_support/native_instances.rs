// SPDX-License-Identifier: AGPL-3.0-or-later
//! Receipt-bound native status only. Generic lifecycle never adopts SCM entries.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::path::Path;
use uuid::Uuid;

#[derive(Deserialize)]
struct Registration {
    schema_version: u32,
    operation_id: Uuid,
    expected_instance_id: Uuid,
    release_identity: String,
    manifest_sha256: String,
    config_fingerprint: String,
    binary_path: String,
}
#[derive(Deserialize)]
struct Journal {
    schema_version: u32,
    operation_id: Uuid,
    action: String,
    phase: String,
    input_sha256: String,
    plan: Plan,
}
#[derive(Deserialize)]
struct Plan {
    name: String,
    service_name: String,
    account: String,
    release_identity: String,
    executable_name: String,
    expected_instance_id: Uuid,
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn named_path(config: &Path, root: &Path) -> Result<Option<String>> {
    let config = config
        .to_str()
        .context("selected service path is not Unicode")?
        .replace('/', "\\")
        .to_lowercase();
    let root = root
        .to_str()
        .context("native installation root is not Unicode")?
        .replace('/', "\\")
        .to_lowercase();
    let Some(relative) = config.strip_prefix(&format!("{root}\\")) else {
        return Ok(None);
    };
    let name = relative.strip_prefix("configurations\\").and_then(|value| value.strip_suffix("\\codifix.toml"))
        .context("selected native service needs its exact installed configuration; no fallback attempted")?;
    ensure!(
        !name.is_empty()
            && name.len() <= 48
            && name.as_bytes()[0].is_ascii_lowercase()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "invalid installed native instance path"
    );
    Ok(Some(name.to_owned()))
}

fn quote(value: &str) -> String {
    let mut output = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            '\\',
            if ch == '"' { slashes * 2 + 1 } else { slashes },
        ));
        output.push(ch);
        slashes = 0;
    }
    output.extend(std::iter::repeat_n('\\', slashes * 2));
    output.push('"');
    output
}

fn validate(
    name: &str,
    root: &Path,
    releases: &Path,
    registration: &Registration,
    journal: &Journal,
) -> Result<()> {
    let plan = &journal.plan;
    let release = &registration.release_identity;
    ensure!(
        registration.schema_version == 1
            && journal.schema_version == 1
            && !registration.operation_id.is_nil()
            && registration.operation_id == journal.operation_id
            && !registration.expected_instance_id.is_nil()
            && registration.expected_instance_id == plan.expected_instance_id
            && journal.phase == "completed"
            && ["Install", "Upgrade"].contains(&journal.action.as_str())
            && plan.name == name
            && plan.service_name == format!("Codifix-{name}")
            && plan.account == format!("NT SERVICE\\Codifix-{name}")
            && plan.executable_name == "codifix.exe"
            && plan.release_identity == *release
            && digest(&registration.manifest_sha256)
            && digest(&registration.config_fingerprint)
            && digest(&journal.input_sha256),
        "selected native instance receipts disagree; no fallback attempted"
    );
    let (version, build) = release
        .strip_prefix("windows-v")
        .and_then(|value| value.rsplit_once("-build."))
        .context("invalid native release receipt")?;
    ensure!(
        release.len() <= 128
            && semver::Version::parse(version).is_ok()
            && build.parse::<u32>().is_ok_and(|value| value > 0)
            && release
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b)),
        "invalid native release receipt"
    );
    let release = releases.join(release).join("bin");
    let args = [
        release
            .join("codifix-windows-service.exe")
            .display()
            .to_string(),
        "--name".into(),
        plan.service_name.clone(),
        "--directory".into(),
        root.join("instances").join(name).display().to_string(),
        "--".into(),
        release.join("codifix.exe").display().to_string(),
        "--config".into(),
        root.join("configurations")
            .join(name)
            .join("codifix.toml")
            .display()
            .to_string(),
        "service".into(),
        "run".into(),
    ];
    let expected = args
        .iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    ensure!(
        registration.binary_path == expected,
        "native command differs from its installed instance binding"
    );
    Ok(())
}

#[cfg(windows)]
fn selection(config: &Path, root: &Path) -> Result<Option<String>> {
    if let Some(name) = named_path(config, root)? {
        return Ok(Some(name));
    }
    // Uninstalled area configs are not permission to create/remove the legacy
    // singleton. This read only narrows authority; it never selects an SCM name.
    if config.exists() {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(config)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 1024 * 1024,
            "selected service config exceeds bounds"
        );
        let value: toml::Value = std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| toml::from_str(text).ok())
            .context("selected service configuration is invalid; no fallback attempted")?;
        ensure!(
            value.get("area").is_none(),
            "selected named area lacks an installed native instance binding; no fallback attempted"
        );
    }
    Ok(None)
}

#[cfg(windows)]
pub(super) fn require_legacy_lifecycle(config: &Path) -> Result<()> {
    let (root, _) = codifix_platform::windows_service_status::installation_roots()?;
    ensure!(
        selection(config, &root)?.is_none(),
        "named native service lifecycle requires its journaled native-instance operator; no legacy task was changed"
    );
    Ok(())
}

#[cfg(windows)]
pub(super) fn status(config: &Path) -> Result<Option<String>> {
    use codifix_platform::{private_windows::Input, windows_service_status};
    let (root, releases) = windows_service_status::installation_roots()?;
    let Some(name) = selection(config, &root)? else {
        return Ok(None);
    };
    ensure!(
        config.is_file(),
        "selected native configuration is unavailable; no fallback attempted"
    );
    let registration_input = Input::open(
        &root.join("registrations").join(format!("{name}.json")),
        1024 * 1024,
    )
    .context("selected native registration is unavailable or untrusted; no fallback attempted")?;
    let registration: Registration = serde_json::from_slice(&registration_input.read(1024 * 1024)?)
        .map_err(|_| anyhow::anyhow!("invalid native registration receipt"))?;
    let journal_input = Input::open(
        &root
            .join("operations")
            .join(registration.operation_id.to_string())
            .join("journal.json"),
        1024 * 1024,
    )
    .context("selected native operation is unavailable or untrusted; no fallback attempted")?;
    let journal: Journal = serde_json::from_slice(&journal_input.read(1024 * 1024)?)
        .map_err(|_| anyhow::anyhow!("invalid native operation receipt"))?;
    validate(&name, &root, &releases, &registration, &journal)?;
    let observed = windows_service_status::observe(&journal.plan.service_name)?;
    ensure!(
        observed.binary_path == registration.binary_path
            && observed.account.eq_ignore_ascii_case(&journal.plan.account),
        "SCM registration differs from the selected native installation receipt; no fallback attempted"
    );
    let state = match observed.state {
        1 => "stopped",
        2 => "start-pending",
        3 => "stop-pending",
        4 => "running",
        5 => "continue-pending",
        6 => "pause-pending",
        7 => "paused",
        _ => "unknown",
    };
    let startup = match observed.start_type {
        2 => "automatic",
        3 => "manual",
        4 => "disabled",
        _ => "unknown",
    };
    Ok(Some(format!(
        "Backend native Windows service:\n- service: {}\n- instance: {}\n- release: {}\n- config: {}\n- registration: verified operator receipts and matching SCM account/command\n- state: {state}\n- startup: {startup}\n- HTTP health: not checked\n- config contents: not revalidated\nLifecycle: use the journaled native-instance operator; no legacy-task fallback.",
        journal.plan.service_name,
        registration.expected_instance_id,
        registration.release_identity,
        config.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_paths_use_exact_installed_locations_not_area_labels() {
        let root = Path::new(r"C:\ProgramData\Codifix");
        assert_eq!(
            named_path(
                Path::new(r"c:\programdata\Codifix\configurations\homelab\codifix.toml"),
                root
            )
            .unwrap()
            .as_deref(),
            Some("homelab")
        );
        assert!(
            named_path(Path::new(r"C:\user\homelab.toml"), root)
                .unwrap()
                .is_none()
        );
        for path in [
            r"C:\ProgramData\Codifix\configurations\..\codifix.toml",
            r"C:\ProgramData\Codifix\other.toml",
        ] {
            assert!(named_path(Path::new(path), root).is_err());
        }
    }
    #[test]
    fn native_receipts_require_completed_exact_instance_and_command() {
        let root = Path::new("/private/Codifix");
        let releases = Path::new("/private/releases");
        let release = "windows-v2.1.0-rc.5-build.1";
        let command = [
            releases
                .join(release)
                .join("bin")
                .join("codifix-windows-service.exe")
                .display()
                .to_string(),
            "--name".into(),
            "Codifix-homelab".into(),
            "--directory".into(),
            root.join("instances").join("homelab").display().to_string(),
            "--".into(),
            releases
                .join(release)
                .join("bin")
                .join("codifix.exe")
                .display()
                .to_string(),
            "--config".into(),
            root.join("configurations")
                .join("homelab")
                .join("codifix.toml")
                .display()
                .to_string(),
            "service".into(),
            "run".into(),
        ]
        .iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
        let registration = serde_json::json!({ "schema_version": 1, "operation_id": Uuid::from_u128(1), "expected_instance_id": Uuid::from_u128(2), "release_identity": release, "manifest_sha256": "a".repeat(64), "config_fingerprint": "b".repeat(64), "binary_path": command });
        let journal = serde_json::json!({ "schema_version": 1, "operation_id": Uuid::from_u128(1), "action": "Install", "phase": "completed", "input_sha256": "c".repeat(64), "plan": { "name": "homelab", "service_name": "Codifix-homelab", "account": "NT SERVICE\\Codifix-homelab", "release_identity": release, "executable_name": "codifix.exe", "expected_instance_id": Uuid::from_u128(2) } });
        let verify = |reg, journal| {
            validate(
                "homelab",
                root,
                releases,
                &serde_json::from_value(reg).unwrap(),
                &serde_json::from_value(journal).unwrap(),
            )
        };
        verify(registration.clone(), journal.clone()).unwrap();
        for field in ["operation_id", "expected_instance_id"] {
            let mut reg = registration.clone();
            reg[field] = serde_json::json!(Uuid::from_u128(99));
            assert!(verify(reg, journal.clone()).is_err());
        }
        for phase in ["prepared", "process_running", "service_configured"] {
            let mut value = journal.clone();
            value["phase"] = phase.into();
            assert!(verify(registration.clone(), value).is_err());
        }
        let mut foreign = registration.clone();
        foreign["binary_path"] = "foreign service command".into();
        assert!(verify(foreign, journal.clone()).is_err());
        for (field, value) in [
            ("service_name", "Codifix-other"),
            ("account", "LocalSystem"),
            ("executable_name", "codifix-federation-gateway.exe"),
            ("name", "other"),
        ] {
            let mut changed = journal.clone();
            changed["plan"][field] = value.into();
            assert!(verify(registration.clone(), changed).is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn unavailable_named_selection_cannot_preview_or_change_legacy_task() {
        let (root, _) = codifix_platform::windows_service_status::installation_roots().unwrap();
        let config = root
            .join("configurations")
            .join(format!("absent-{}", Uuid::new_v4().simple()))
            .join("codifix.toml");
        assert!(
            require_legacy_lifecycle(&config)
                .unwrap_err()
                .to_string()
                .contains("journaled native-instance operator")
        );
        assert!(status(&config).is_err());
        assert!(
            super::super::preview_enable_backend_service(&config).starts_with("Dry run rejected:")
        );
        assert!(
            super::super::preview_disable_backend_service(&config).starts_with("Dry run rejected:")
        );
    }

    #[cfg(windows)]
    #[test]
    fn uninstalled_area_config_does_not_guess_an_scm_name() {
        let directory = std::env::temp_dir().join(format!("native-selection-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let config = directory.join("homelab.toml");
        std::fs::write(&config, "[area]\nname = 'homelab'\n").unwrap();
        let (root, _) = codifix_platform::windows_service_status::installation_roots().unwrap();
        assert!(
            selection(&config, &root)
                .unwrap_err()
                .to_string()
                .contains("no fallback attempted")
        );
        std::fs::write(&config, "[service]\nhost = '127.0.0.1'\n").unwrap();
        assert!(selection(&config, &root).unwrap().is_none());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
