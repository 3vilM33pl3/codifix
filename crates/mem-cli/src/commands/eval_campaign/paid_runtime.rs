// SPDX-License-Identifier: AGPL-3.0-or-later
//! Extra entry gate for the trusted provider controller. Approval and provider
//! budgets remain enforced by the normal runner and per-call owner.
use super::*;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    path: String,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Runtime {
    version: u32,
    profile: String,
    root: std::path::PathBuf,
    files: Vec<Pin>,
    binaries: BTreeMap<String, Pin>,
    images: BTreeMap<String, String>,
}

pub(super) fn verify(root: &Path, file: &Path, driver: &Path, manifest: &Manifest) -> Result<()> {
    ensure!(
        file == root.join("private/paid-runtime.json"),
        "unexpected approved runtime path"
    );
    ensure!(
        manifest
            .provider_profile
            .as_ref()
            .is_some_and(|p| p.version == 2)
            && manifest.provider_budget.is_some()
            && manifest.preparation_budget.is_some(),
        "approved runtime requires v2 provider budgets"
    );
    let bytes = std::fs::read(file)?;
    ensure!(bytes.len() <= 65536, "oversized approved runtime");
    ensure!(
        manifest.assets.iter().any(|a| a.path
            == std::path::PathBuf::from("private/paid-runtime.json")
            && a.kind == c::AssetKind::Configuration
            && a.sha256 == c::digest(&bytes)),
        "unlocked approved runtime"
    );
    let runtime: Runtime = serde_json::from_slice(&bytes)?;
    ensure!(
        runtime.version == 1 && runtime.profile == "memory-r3-approved-v2" && runtime.root == root,
        "approved runtime identity mismatch"
    );
    ensure!(
        runtime
            .binaries
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            == [
                "bao",
                "budget",
                "memory",
                "node",
                "retriever",
                "score",
                "snapshot"
            ],
        "incomplete approved executables"
    );
    ensure!(
        runtime
            .images
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            == ["node", "rust"]
            && runtime.images.values().all(|id| id.len() == 71
                && id.starts_with("sha256:")
                && id[7..]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
        "immutable approved images required"
    );
    ensure!(
        !runtime.files.is_empty() && runtime.files.len() <= 64,
        "invalid approved runtime files"
    );
    for pin in &runtime.files {
        ensure!(
            pin.path.starts_with("runtime/") && c::relative_path(Path::new(&pin.path)),
            "invalid runtime asset path"
        );
        ensure!(
            manifest
                .assets
                .iter()
                .any(|a| a.kind == c::AssetKind::Configuration
                    && a.path == Path::new(&pin.path)
                    && a.sha256 == pin.sha256),
            "runtime source is not locked"
        );
    }
    ensure!(
        driver == root.join("runtime/approved-driver.mjs")
            && runtime
                .files
                .iter()
                .any(|pin| pin.path == "runtime/approved-driver.mjs")
            && runtime
                .files
                .iter()
                .any(|pin| pin.path == "runtime/launch-paid.mjs"),
        "unexpected approved driver"
    );
    for (name, pin) in &runtime.binaries {
        ensure!(
            pin.path == format!("private/executables/{name}"),
            "invalid approved executable path"
        );
        let path = root.join(&pin.path);
        ensure!(
            std::fs::symlink_metadata(&path)?.is_file() && path.canonicalize()?.starts_with(root),
            "invalid approved executable"
        );
        ensure!(
            c::digest(&std::fs::read(path)?) == pin.sha256,
            "approved executable changed"
        );
    }
    ensure!(
        c::digest(&std::fs::read(std::env::current_exe()?)?) == runtime.binaries["memory"].sha256,
        "runner differs from approved executable"
    );
    Ok(())
}
