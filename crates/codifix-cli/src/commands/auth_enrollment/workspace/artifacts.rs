// SPDX-License-Identifier: AGPL-3.0-or-later
//! The reviewed Memory skill-bundle ZIP supplies Codex and Claude project skills.
//! No executable plugin install, hook, generic extraction or auth reuse.
use super::{Journal, filesystem};
use anyhow::{Result, bail};
use codifix_record::OnboardingPinnedTool;
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read},
    path::{Component, Path},
    time::Duration,
};
use zip::ZipArchive;

const MAX_DOWNLOAD: usize = 16 * 1024 * 1024;
const MAX_FILE: usize = 2 * 1024 * 1024;

fn skill_version_matches(actual: &str, pinned: &str) -> bool {
    if actual == pinned {
        return true;
    }
    // Release pins may carry a build label absent from skill frontmatter.
    // Match the entire remaining SemVer string, including its prerelease.
    // The archive SHA-256 remains the exact content identity.
    semver::Version::parse(pinned).is_ok_and(|version| {
        !version.build.is_empty()
            && pinned
                .split_once('+')
                .is_some_and(|(version, _)| actual == version)
    })
}
pub(super) trait Downloads {
    fn fetch(&self, url: &str) -> impl std::future::Future<Output = Result<Vec<u8>>>;
}
pub(super) struct HttpsArtifacts(reqwest::Client);
impl HttpsArtifacts {
    pub(super) fn new() -> Result<Self> {
        Ok(Self(
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|_| anyhow::anyhow!("artifact client unavailable"))?,
        ))
    }
}
impl Downloads for HttpsArtifacts {
    async fn fetch(&self, url: &str) -> Result<Vec<u8>> {
        filesystem::https_url(url)?;
        let mut response = self.0.get(url).send().await.map_err(|_| {
            anyhow::anyhow!("pinned artifact download unconfirmed; no credentials were sent")
        })?;
        if !response.status().is_success() {
            bail!("pinned artifact request denied; response body suppressed");
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("pinned artifact download interrupted"))?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_DOWNLOAD {
                bail!("pinned artifact exceeds 16 MiB limit");
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

type Bundle = BTreeMap<String, BTreeMap<String, Vec<u8>>>;
// Track the materialized tree, not just complete ZIP names: case-insensitive
// filesystems also alias intermediate directories and file/directory prefixes.
fn register(
    tree: &mut BTreeMap<String, (String, bool)>,
    parts: &[&str],
    directory: bool,
) -> Result<()> {
    for index in 1..=parts.len() {
        let spelling = parts[..index].join("/");
        let key = spelling.to_ascii_lowercase();
        let is_directory = index < parts.len() || directory;
        if let Some((original, prior_directory)) = tree.get(&key) {
            if original != &spelling || !is_directory || !prior_directory {
                bail!(
                    "archive has case aliases, duplicate files or file/directory prefix conflicts"
                );
            }
        } else {
            tree.insert(key, (spelling, is_directory));
        }
    }
    Ok(())
}
pub(super) fn bundle(bytes: &[u8]) -> Result<Bundle> {
    if bytes.len() > MAX_DOWNLOAD {
        bail!("pinned archive exceeds download limit");
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| {
        anyhow::anyhow!("unsupported memory-skills format; expected a Memory skill ZIP bundle")
    })?;
    if archive.len() > 4096 {
        bail!("pinned archive has too many entries");
    }
    let mut files: Bundle = BTreeMap::new();
    let mut tree = BTreeMap::new();
    let mut total = 0usize;
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        let path = file
            .enclosed_name()
            .ok_or_else(|| anyhow::anyhow!("unsafe pinned archive path"))?;
        let parts = path
            .components()
            .map(|part| match part {
                Component::Normal(value) => value
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("archive path must be UTF-8")),
                _ => Err(anyhow::anyhow!("unsafe archive traversal")),
            })
            .collect::<Result<Vec<_>>>()?;
        if parts.iter().any(|part| {
            part.len() > 128
                || part
                    .chars()
                    .any(|c| c.is_control() || c == ':' || c == '\\')
        }) {
            bail!("unsafe pinned archive name");
        }
        let kind = file.unix_mode().unwrap_or(0) & 0o170000;
        if !matches!(kind, 0 | 0o100000 | 0o040000) {
            bail!("archive symlinks/special entries are unsupported");
        }
        total = total.saturating_add(usize::try_from(file.size()).unwrap_or(usize::MAX));
        if file.size() > MAX_FILE as u64 || total > 64 * 1024 * 1024 {
            bail!("pinned archive expansion exceeds safe bounds");
        }
        let Some(offset) = parts.windows(2).position(|parts| {
            parts[0].eq_ignore_ascii_case(".agents") && parts[1].eq_ignore_ascii_case("skills")
        }) else {
            continue;
        };
        let selected = &parts[offset + 2..];
        if selected.is_empty()
            || !codifix_skills::MEMORY_SKILL_NAMES
                .iter()
                .any(|name| name.eq_ignore_ascii_case(selected[0]))
        {
            continue;
        }
        if parts[offset..offset + 2] != [".agents", "skills"]
            || !codifix_skills::MEMORY_SKILL_NAMES.contains(&selected[0])
        {
            bail!("archive skill root has a case alias");
        }
        if file
            .name()?
            .trim_end_matches('/')
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
        {
            bail!("selected archive path has an ambiguous prefix");
        }
        if selected.iter().any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        }) {
            bail!("selected archive names require bounded minimal ASCII components");
        }
        if selected
            .iter()
            .any(|part| part.eq_ignore_ascii_case(".memory-onboarding-pin.json"))
        {
            bail!("archive collides with Memory install receipt");
        }
        register(&mut tree, selected, file.is_dir())?;
        if file.is_dir() {
            continue;
        }
        if selected.len() < 2 {
            bail!("archive skill root must be a directory");
        }
        let relative = selected[1..].join("/");
        let mut content = Vec::new();
        file.take(MAX_FILE as u64 + 1).read_to_end(&mut content)?;
        if content.len() > MAX_FILE {
            bail!("archive file exceeds size limit");
        }
        if files
            .entry(selected[0].into())
            .or_default()
            .insert(relative, content)
            .is_some()
        {
            bail!("duplicate skill archive entry");
        }
    }
    for name in codifix_skills::MEMORY_SKILL_NAMES {
        if !files
            .get(*name)
            .is_some_and(|files| files.contains_key("SKILL.md"))
        {
            bail!("pinned Memory skill bundle is incomplete");
        }
    }
    Ok(files)
}

pub(super) async fn apply(
    path: &Path,
    journal: &mut Journal,
    tool: &OnboardingPinnedTool,
    downloads: &impl Downloads,
) -> Result<()> {
    filesystem::https_url(&tool.url)?;
    let cache = path.with_file_name(format!(".memory-artifacts-{}", journal.request_id));
    filesystem::owned_directory(&cache)?;
    let pin = tool.sha256.to_ascii_lowercase();
    let archive = cache.join(format!("{pin}.zip"));
    let bytes = if filesystem::exists(&archive)? {
        filesystem::absolute_leaf(&archive)?;
        let mut bytes = Vec::new();
        fs::File::open(&archive)?
            .take(MAX_DOWNLOAD as u64 + 1)
            .read_to_end(&mut bytes)?;
        bytes
    } else {
        let bytes = downloads.fetch(&tool.url).await?;
        if bytes.len() > MAX_DOWNLOAD || filesystem::bytes_digest(&bytes) != pin {
            bail!("pinned artifact SHA-256 mismatch; nothing installed");
        }
        filesystem::install_missing(&cache, &format!("{pin}.zip"), &bytes)?;
        bytes
    };
    if bytes.len() > MAX_DOWNLOAD || filesystem::bytes_digest(&bytes) != pin {
        bail!("cached pinned artifact SHA-256 mismatch; nothing installed");
    }
    let files = bundle(&bytes)?;
    // Validate all versions before applying even the first skill.
    for (name, contents) in &files {
        let stage = cache.join(format!("{pin}-{name}"));
        filesystem::owned_directory(&stage)?;
        for (relative, bytes) in contents {
            if !filesystem::install_missing(&stage, relative, bytes)? {
                let existing = stage.join(relative);
                filesystem::absolute_leaf(&existing)?;
                let mut content = Vec::new();
                fs::File::open(existing)?
                    .take(MAX_FILE as u64 + 1)
                    .read_to_end(&mut content)?;
                if content != *bytes {
                    bail!("skill staging inputs changed; no replacement performed");
                }
            }
        }
        if !codifix_skills::read_skill_version(&stage)?
            .as_deref()
            .is_some_and(|actual| skill_version_matches(actual, &tool.version))
        {
            bail!("skill version differs from its reviewed pin; nothing installed");
        }
    }
    for (name, contents) in &files {
        for root in [".agents", ".claude"] {
            publish_skill(path, journal, &cache, &pin, name, contents, tool, root)?;
        }
    }
    filesystem::save(path, journal)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn publish_skill(
    path: &Path,
    journal: &mut Journal,
    cache: &Path,
    pin: &str,
    name: &str,
    contents: &BTreeMap<String, Vec<u8>>,
    tool: &OnboardingPinnedTool,
    root: &str,
) -> Result<()> {
    let relative = format!("{root}/skills/{name}");
    let target = journal.plan.workspace.join(&relative);
    let receipt = serde_json::to_vec(
        &serde_json::json!({"request_id":journal.request_id,"sha256":pin,"version":tool.version}),
    )?;
    // A distinct staged tree for each destination allows interruption after
    // the first publication without moving/copying an existing user tree.
    let stage = cache.join(format!("{pin}-{name}-{root}"));
    filesystem::owned_directory(&stage)?;
    for (relative, bytes) in contents {
        if !filesystem::install_missing(&stage, relative, bytes)? {
            let existing = stage.join(relative);
            filesystem::absolute_leaf(&existing)?;
            let mut content = Vec::new();
            fs::File::open(existing)?
                .take(MAX_FILE as u64 + 1)
                .read_to_end(&mut content)?;
            if content != *bytes {
                bail!("agent-specific skill staging inputs changed; no replacement performed");
            }
        }
    }
    filesystem::install_missing(&stage, ".memory-onboarding-pin.json", &receipt)?;
    if filesystem::exists(&target)? {
        if let Some(prior) = filesystem::receipt_bytes(&target, ".memory-onboarding-pin.json")? {
            if prior == receipt {
                filesystem::publish_directory(
                    &stage,
                    &target,
                    ".memory-onboarding-pin.json",
                    &receipt,
                )?;
                return Ok(());
            }
            #[derive(serde::Deserialize)]
            struct PublicationOwner {
                request_id: uuid::Uuid,
            }
            let prior: PublicationOwner = serde_json::from_slice(&prior)
                .map_err(|_| anyhow::anyhow!("managed skill publication receipt is invalid"))?;
            if prior.request_id == journal.request_id || prior.request_id.is_nil() {
                bail!("managed publication belongs to different immutable inputs");
            }
            // Another request's installed skills are existing project content.
            // Preserve both content and receipt; never adopt or repair them.
        }
        if !journal.preserved_paths.contains(&relative) {
            journal.preserved_paths.push(relative);
        }
        filesystem::save(path, journal)?;
        return Ok(());
    }
    // Publish a fully validated private directory. An interruption before
    // publication keeps staging; after publication keeps complete skills.
    // No file is executable and no script/plugin is invoked.
    filesystem::owned_directory(&journal.plan.workspace.join(root))?;
    filesystem::owned_directory(&journal.plan.workspace.join(format!("{root}/skills")))?;
    if filesystem::exists(&target)? {
        bail!("skill destination appeared during apply; staging retained");
    }
    filesystem::publish_directory(&stage, &target, ".memory-onboarding-pin.json", &receipt)?;
    filesystem::save(path, journal)?;
    Ok(())
}

#[cfg(test)]
mod version_tests {
    use super::skill_version_matches;

    #[test]
    fn reviewed_build_label_can_be_absent_from_skill_frontmatter() {
        for (actual, pinned) in [
            ("2.1.0-rc.5", "2.1.0-rc.5+89d52aa8"),
            ("2.1.0", "2.1.0+build.89d52aa8"),
            ("2.1.0-rc.5+89d52aa8", "2.1.0-rc.5+89d52aa8"),
            ("2.1.0-rc.5", "2.1.0-rc.5"),
        ] {
            assert!(skill_version_matches(actual, pinned), "{actual} / {pinned}");
        }
    }

    #[test]
    fn build_label_compatibility_never_relaxes_version_or_existing_build_identity() {
        for (actual, pinned) in [
            ("2.1.0-rc.4", "2.1.0-rc.5+89d52aa8"),
            ("2.1.0", "2.1.0-rc.5+89d52aa8"),
            ("2.1.1-rc.5", "2.1.0-rc.5+89d52aa8"),
            ("2.1.0-rc.5+other", "2.1.0-rc.5+89d52aa8"),
            ("2.1.0-rc.5+89d52aa8", "2.1.0-rc.5"),
            ("2.1.0-rc.5", "^2.1.0-rc.5+89d52aa8"),
            ("2.1.0-rc.5", "2.1.0-rc.5+"),
            ("v2.1.0-rc.5", "v2.1.0-rc.5+89d52aa8"),
        ] {
            assert!(
                !skill_version_matches(actual, pinned),
                "{actual} / {pinned}"
            );
        }
    }
}
