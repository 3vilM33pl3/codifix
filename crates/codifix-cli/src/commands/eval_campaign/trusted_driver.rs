// SPDX-License-Identifier: AGPL-3.0-or-later
//! Trusted harness launch only. This does not isolate agents or validate the
//! native evaluation backend; those remain separate acceptance requirements.
use super::*;
use std::{fs::File, time::Duration};

#[cfg(any(windows, test))]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Runtime {
    schema_version: u32,
    node: std::path::PathBuf,
    node_sha256: String,
}
#[cfg(any(windows, test))]
impl Runtime {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && !self.node.as_os_str().is_empty()
                && !self.node.is_absolute()
                && self
                    .node
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
                && self.node_sha256.len() == 64
                && self
                    .node_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid Windows Node runtime descriptor"
        );
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    manifest_path: &Path,
    manifest: &Manifest,
    driver: &Path,
    driver_hash: &str,
    dir: &Path,
    offline: bool,
    seconds: u64,
    stdout: File,
    stderr: File,
) -> Result<bool> {
    #[cfg(not(windows))]
    {
        let _ = (manifest_path, manifest, driver_hash);
        let mut command = tokio::process::Command::new(driver);
        command
            .arg(dir.join("request.json"))
            .current_dir(dir)
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("HOME", dir.join("home"))
            .env("CODIFIX_PROFILE", "prod")
            .env("MEMORY_R3_OFFLINE", if offline { "1" } else { "0" })
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .kill_on_drop(true);
        Ok(tokio::time::timeout(Duration::from_secs(seconds), command.status()).await
            .context("driver deadline expired; retain uncertain evidence and reconcile owned containers")??.success())
    }
    #[cfg(windows)]
    {
        use codifix_platform::{
            private_windows::{Directory, Input},
            windows_trusted_process,
        };
        use std::{ffi::OsString, io::Read};
        let root = manifest_path.parent().context("manifest directory")?;
        let sidecar = driver.with_file_name(format!(
            "{}.windows-runtime.json",
            driver
                .file_name()
                .context("driver filename")?
                .to_str()
                .context("driver filename encoding")?
        ));
        let pin = |path: &Path, expected: Option<&str>, limit| -> Result<(Input, Vec<u8>)> {
            let input = Input::open(path, limit)?;
            let mut bytes = Vec::new();
            input
                .file
                .try_clone()?
                .take(limit + 1)
                .read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() as u64 <= limit,
                "oversized trusted launch input"
            );
            input.unchanged()?;
            let hash = c::digest(&bytes);
            ensure!(
                expected.is_none_or(|expected| expected == hash),
                "trusted launch input changed"
            );
            ensure!(
                manifest
                    .assets
                    .iter()
                    .any(|asset| asset.kind == c::AssetKind::Configuration
                        && asset.sha256 == hash
                        && root.join(&asset.path).canonicalize().ok().as_deref() == Some(path)),
                "Windows launch input is not a locked configuration asset"
            );
            Ok((input, bytes))
        };
        let (descriptor_pin, bytes) = pin(&sidecar, None, 16384)?;
        let runtime: Runtime = serde_json::from_slice(&bytes)?;
        runtime.validate()?;
        let node = root.join(&runtime.node).canonicalize()?;
        ensure!(
            node.starts_with(root)
                && node
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("node.exe")),
            "Node must be a campaign-local native node.exe"
        );
        ensure!(
            driver
                .extension()
                .is_some_and(|v| v == "mjs" || v == "js" || v == "cjs"),
            "Windows campaign driver must be an explicit Node script"
        );
        let (node_pin, _) = pin(&node, Some(&runtime.node_sha256), 256 * 1024 * 1024)?;
        let (script_pin, _) = pin(driver, Some(driver_hash), 16 * 1024 * 1024)?;
        let directory_pin = Directory::open(dir)?;
        let home_pin = Directory::open(&dir.join("home"))?;
        let temp = dir.join("temp");
        codifix_platform::private_windows::create_private_directory(&temp)?;
        let temp_pin = Directory::open(&temp)?;
        let mut system = vec![0u16; 32768];
        let count = unsafe {
            windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW(
                system.as_mut_ptr(),
                system.len() as u32,
            )
        } as usize;
        ensure!(
            count > 0 && count < system.len(),
            "Windows directory unavailable"
        );
        use std::os::windows::ffi::OsStringExt;
        let system = OsString::from_wide(&system[..count]);
        let env = vec![
            ("SystemRoot".into(), system),
            ("HOME".into(), dir.join("home").into_os_string()),
            ("USERPROFILE".into(), dir.join("home").into_os_string()),
            ("TEMP".into(), temp.clone().into_os_string()),
            ("TMP".into(), temp.into_os_string()),
            (
                "PATH".into(),
                node.parent()
                    .context("Node directory")?
                    .as_os_str()
                    .to_owned(),
            ),
            ("CODIFIX_PROFILE".into(), "prod".into()),
            (
                "MEMORY_R3_OFFLINE".into(),
                if offline { "1" } else { "0" }.into(),
            ),
        ];
        let process = windows_trusted_process::spawn(
            &node,
            &[
                driver.as_os_str().to_owned(),
                dir.join("request.json").into_os_string(),
            ],
            dir,
            &env,
            &stdout,
            &stderr,
        )?;
        let code = tokio::time::timeout(Duration::from_secs(seconds), async {
            loop {
                if let Some(code) = process.try_wait()? {
                    break Ok::<_, anyhow::Error>(code);
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .context("native driver deadline expired; Job cancelled, retain uncertain evidence")??;
        // Quiesce the entire Job before accepting output. A driver that exits
        // while leaving writers alive cannot race observation acceptance.
        process.terminate()?;
        tokio::time::timeout(Duration::from_secs(5), async {
            while process.active_processes()? != 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("native driver descendants not reaped; evidence not accepted")??;
        drop(process);
        for input in [&descriptor_pin, &node_pin, &script_pin] {
            input.unchanged()?;
        }
        let _ = (directory_pin, home_pin, temp_pin);
        Ok(code == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_runtime_requires_exact_schema_relative_node_and_digest() {
        let fixture = || Runtime {
            schema_version: 1,
            node: "runtime/node.exe".into(),
            node_sha256: "a".repeat(64),
        };
        assert!(fixture().validate().is_ok());
        for path in ["", "../node.exe", "/node.exe"] {
            let mut runtime = fixture();
            runtime.node = path.into();
            assert!(runtime.validate().is_err());
        }
        let mut runtime = fixture();
        runtime.schema_version = 2;
        assert!(runtime.validate().is_err());
        runtime = fixture();
        runtime.node_sha256 = "untrusted".into();
        assert!(runtime.validate().is_err());
        assert!(serde_json::from_value::<Runtime>(serde_json::json!({"schema_version":1,"node":"node.exe","node_sha256":"a".repeat(64),"args":["--eval"]})).is_err());
    }
}
