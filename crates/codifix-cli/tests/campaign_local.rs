// SPDX-License-Identifier: AGPL-3.0-or-later
use std::{fs, path::PathBuf, process::Command};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn only_local_campaign_checks_ignore_service_configuration() {
    let root =
        Scratch(std::env::temp_dir().join(format!("campaign-local-{}", uuid::Uuid::new_v4())));
    fs::create_dir(&root.0).expect("create isolated campaign directory");
    let manifest = root.0.join("manifest.json");
    fs::write(&manifest, "!").expect("write invalid campaign manifest");
    let invoke = |name: &str| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codifix"));
        command
            .env_clear()
            .env("HOME", &root.0)
            .env("USERPROFILE", &root.0)
            .env("XDG_CONFIG_HOME", root.0.join("config"))
            .env("APPDATA", root.0.join("roaming"))
            .env("LOCALAPPDATA", root.0.join("local"))
            .env("CODIFIX_PROFILE", "prod")
            .current_dir(&root.0)
            .arg("--config")
            .arg(root.0.join("missing-service.toml"))
            .args(["eval", "campaign", name, "--manifest"])
            .arg(&manifest);
        if name == "report" {
            command
                .arg("--artifact")
                .arg(root.0.join("missing-artifact.json"));
        }
        command.output().expect("run campaign validation command")
    };
    for name in ["plan", "check-approvals"] {
        let result = invoke(name);
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        // Reach the real manifest parser rather than an ambient config/client.
        assert!(
            error.contains("expected value at line 1 column 1"),
            "{name}: {error}"
        );
        assert!(!error.contains("load config"), "{name}: {error}");
    }
    let result = invoke("report");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("load config"));
}
