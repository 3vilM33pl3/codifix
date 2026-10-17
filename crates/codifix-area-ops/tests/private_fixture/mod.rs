// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native private fixtures, including equivalent negative permission boundaries.
#![allow(dead_code)]
use std::{fs, io::Write, path::Path};

pub struct PrivateDir {
    _outer: tempfile::TempDir,
    path: std::path::PathBuf,
}
impl PrivateDir {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn private_dir() -> PrivateDir {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let outer = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .expect("private fixture");
        PrivateDir {
            path: outer.path().to_owned(),
            _outer: outer,
        }
    }
    #[cfg(windows)]
    {
        // Keep TempDir ownership of the outer container, but all operator data
        // lives in a child created with the production protected DACL.
        let outer = tempfile::tempdir().expect("fixture container");
        let path = outer.path().join("private");
        codifix_platform_files::windows::create_private_directory(&path)
            .expect("private directory");
        PrivateDir {
            _outer: outer,
            path,
        }
    }
}

pub fn write(path: &Path, bytes: &[u8]) {
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .expect("private file")
    };
    #[cfg(windows)]
    let mut file =
        codifix_platform_files::windows::create_private_file(path).expect("private file");
    file.write_all(bytes).expect("fixture bytes");
}

pub fn replace(path: &Path, bytes: &[u8]) {
    if path.exists() {
        fs::remove_file(path).expect("remove fixture file");
    }
    write(path, bytes);
}

pub fn assert_private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).expect("metadata").permissions().mode() & 0o777,
            0o600
        );
    }
    #[cfg(windows)]
    {
        codifix_platform_files::windows::Input::open(path, 64 * 1024 * 1024)
            .expect("protected output");
    }
}

pub fn public_read(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if path.is_dir() { 0o750 } else { 0o644 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("public fixture");
    }
    #[cfg(windows)]
    {
        acl(path, "/grant", "*S-1-1-0:R");
    }
}

pub fn restore_private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).expect("restore mode");
    }
    #[cfg(windows)]
    {
        acl(path, "/remove:g", "*S-1-1-0");
    }
}

#[cfg(windows)]
fn acl(path: &Path, action: &str, principal: &str) {
    let output = std::process::Command::new("icacls.exe")
        .arg(path)
        .args([action, principal])
        .output()
        .expect("fixture ACL command");
    assert!(output.status.success(), "fixture ACL update failed");
}

pub fn directory_alias(target: &Path, alias: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, alias).expect("directory symlink");
    #[cfg(windows)]
    {
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(alias)
            .arg(target)
            .output()
            .expect("junction command");
        assert!(output.status.success(), "fixture junction failed");
    }
}

pub fn remove_directory_alias(alias: &Path) {
    #[cfg(unix)]
    fs::remove_file(alias).expect("remove symlink");
    #[cfg(windows)]
    fs::remove_dir(alias).expect("remove junction only");
}
