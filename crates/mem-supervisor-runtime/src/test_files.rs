//! Synthetic authority fixtures with the actual platform protection policy.
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub(crate) fn directory() -> tempfile::TempDir {
    #[cfg(unix)]
    {
        tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .expect("private fixture")
    }
    #[cfg(windows)]
    {
        let result = tempfile::tempdir().expect("temporary fixture");
        std::fs::remove_dir(result.path()).expect("empty fixture");
        mem_platform_files::windows::create_private_directory(result.path())
            .expect("private fixture");
        result
    }
}
pub(crate) fn write(path: &Path, bytes: &[u8]) {
    #[cfg(windows)]
    if !path.exists() {
        drop(mem_platform_files::windows::create_private_file(path).expect("private fixture file"));
    }
    std::fs::write(path, bytes).expect("synthetic fixture bytes");
    #[cfg(unix)]
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .expect("private fixture");
}
