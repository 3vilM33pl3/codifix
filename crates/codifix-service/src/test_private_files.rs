// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cross-platform private fixtures; use the real protected Windows primitives.
use std::{io::Write, path::Path};

pub(crate) fn directory(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(path).unwrap();
    }
    #[cfg(windows)]
    codifix_platform::private_windows::create_private_directory(path).unwrap();
}

pub(crate) fn write(path: &Path, bytes: &[u8]) {
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .unwrap()
    };
    #[cfg(windows)]
    let mut file = codifix_platform::private_windows::create_private_file(path).unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    #[cfg(windows)]
    codifix_platform::private_windows::Input::open(path, bytes.len() as u64).unwrap();
}

#[test]
fn private_fixture_preserves_bytes_and_rejects_duplicate_creation() {
    let root =
        std::env::temp_dir().join(format!("memory-private-fixture-{}", uuid::Uuid::new_v4()));
    directory(&root);
    let path = root.join("input.json");
    write(&path, b"synthetic-only");
    assert_eq!(std::fs::read(&path).unwrap(), b"synthetic-only");
    assert!(std::panic::catch_unwind(|| write(&path, b"changed")).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"synthetic-only");
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(root).unwrap();
}
