// SPDX-License-Identifier: AGPL-3.0-or-later
//! Only the existing Memory skill-bundle ZIP layout is supported. No executable
//! plugin install, script invocation, generic archive extraction or auth reuse.
use super::{Journal, filesystem};
use anyhow::{Result, bail};
use mem_record::OnboardingPinnedTool;
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
        if file.is_dir() {
            continue;
        }
        let Some(offset) = parts
            .windows(2)
            .position(|parts| parts == [".agents", "skills"])
        else {
            continue;
        };
        let selected = &parts[offset + 2..];
        if selected.len() < 2 || !mem_skills::MEMORY_SKILL_NAMES.contains(&selected[0]) {
            continue;
        }
        let relative = selected[1..].join("/");
        if relative == ".memory-onboarding-pin.json" {
            bail!("archive collides with Memory install receipt");
        }
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
    for name in mem_skills::MEMORY_SKILL_NAMES {
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
        if mem_skills::read_skill_version(&stage)?.as_deref() != Some(tool.version.as_str()) {
            bail!("skill version differs from its reviewed pin; nothing installed");
        }
    }
    for (name, _) in files {
        let relative = format!(".agents/skills/{name}");
        let target = journal.plan.workspace.join(&relative);
        let receipt = serde_json::to_vec(
            &serde_json::json!({"request_id":journal.request_id,"sha256":pin,"version":tool.version}),
        )?;
        if filesystem::exists(&target)? {
            if filesystem::receipt_matches(&target, ".memory-onboarding-pin.json", &receipt)? {
                let stage = cache.join(format!("{pin}-{name}"));
                filesystem::publish_directory(
                    &stage,
                    &target,
                    ".memory-onboarding-pin.json",
                    &receipt,
                )?;
                continue;
            }
            if !journal.preserved_paths.contains(&relative) {
                journal.preserved_paths.push(relative);
            }
            continue;
        }
        // Publish a fully validated private directory. An interruption before
        // publication keeps staging; after publication keeps complete skills.
        // No file is executable and no script/plugin is invoked.
        let stage = cache.join(format!("{pin}-{name}"));
        filesystem::install_missing(&stage, ".memory-onboarding-pin.json", &receipt)?;
        filesystem::owned_directory(&journal.plan.workspace.join(".agents"))?;
        filesystem::owned_directory(&journal.plan.workspace.join(".agents/skills"))?;
        if filesystem::exists(&target)? {
            bail!("skill destination appeared during apply; staging retained");
        }
        filesystem::publish_directory(&stage, &target, ".memory-onboarding-pin.json", &receipt)?;
    }
    filesystem::save(path, journal)?;
    Ok(())
}
