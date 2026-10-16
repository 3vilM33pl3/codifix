// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tiny ownership boundary for Memory's parent-whitelisted inherited pipe.
//!
//! This does not authenticate a broker. Only the trusted launcher may supply the
//! numeric inherited handle. The input is never adopted, borrowed or closed;
//! only a successful, non-inheritable duplicate becomes Rust-owned. Keeping this
//! boundary separate preserves the pinned Pi crate's `forbid(unsafe_code)`.
#![cfg(windows)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;

type Handle = *mut c_void;
const DUPLICATE_SAME_ACCESS: u32 = 2;
const FILE_TYPE_PIPE: u32 = 3;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn DuplicateHandle(
        source_process: Handle,
        source: Handle,
        target_process: Handle,
        target: *mut Handle,
        access: u32,
        inherit: i32,
        options: u32,
    ) -> i32;
    fn GetFileType(handle: Handle) -> u32;
    fn GetNamedPipeInfo(
        handle: Handle,
        flags: *mut u32,
        out_buffer_size: *mut u32,
        in_buffer_size: *mut u32,
        max_instances: *mut u32,
    ) -> i32;
}

/// Duplicate a trusted-launcher inherited pipe without taking its ownership.
///
/// Rejects null/pseudo values, invalid/closed handles and non-pipe objects.
/// Does not open a named pipe or provide network/name-based fallback. The source
/// handle must come from the parent's explicit inherited-handle whitelist, not
/// model output or untrusted request data. Type checks are not authentication.
pub fn duplicate_pipe(raw: usize) -> io::Result<File> {
    if raw == 0 || raw > isize::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid pipe handle",
        ));
    }
    let mut duplicate = ptr::null_mut();
    // SAFETY: input is passed only as an opaque kernel handle, never dereferenced
    // or converted to BorrowedHandle. The output pointer is valid writable local
    // storage. Both process pseudo-handles refer to this process. inherit=false;
    // SAME_ACCESS alone never closes the original, including on failure.
    let result = unsafe {
        let process = GetCurrentProcess();
        DuplicateHandle(
            process,
            raw as Handle,
            process,
            &mut duplicate,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful DuplicateHandle returned a new, uniquely owned handle
    // in this process. From this point every rejection drops only the duplicate.
    let owned = unsafe { OwnedHandle::from_raw_handle(duplicate) };
    // SAFETY: owned keeps the duplicate live across both Win32 queries. All null
    // optional output pointers are permitted by GetNamedPipeInfo. FILE_TYPE_PIPE
    // alone also describes sockets; the second query requires an actual pipe.
    let is_pipe = unsafe {
        GetFileType(owned.as_raw_handle()) == FILE_TYPE_PIPE
            && GetNamedPipeInfo(
                owned.as_raw_handle(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            ) != 0
    };
    if !is_pipe {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "inherited handle is not a pipe",
        ));
    }
    Ok(File::from(owned))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    const HANDLE_FLAG_INHERIT: u32 = 1;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreatePipe(
            read: *mut Handle,
            write: *mut Handle,
            attributes: *const c_void,
            size: u32,
        ) -> i32;
        fn GetHandleInformation(handle: Handle, flags: *mut u32) -> i32;
        fn SetHandleInformation(handle: Handle, mask: u32, flags: u32) -> i32;
    }
    fn pipe() -> (File, File) {
        let mut read = ptr::null_mut();
        let mut write = ptr::null_mut();
        // SAFETY: valid output storage; null attributes are documented defaults.
        assert_ne!(
            unsafe { CreatePipe(&mut read, &mut write, ptr::null(), 0) },
            0
        );
        // SAFETY: successful CreatePipe transferred two new, unique handles.
        unsafe { (File::from_raw_handle(read), File::from_raw_handle(write)) }
    }
    #[test]
    fn rejects_null_and_pseudo_handles() {
        for raw in [0, usize::MAX, usize::MAX - 1, usize::MAX - 2] {
            assert!(duplicate_pipe(raw).is_err());
        }
    }
    #[test]
    fn rejects_closed_handle() {
        let (read, _write) = pipe();
        let raw = read.as_raw_handle() as usize;
        drop(read);
        assert!(duplicate_pipe(raw).is_err());
    }
    #[test]
    fn rejects_disk_without_closing_original() {
        let path = std::env::temp_dir().join(format!(
            "memory-pipe-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert!(duplicate_pipe(file.as_raw_handle() as usize).is_err());
        file.write_all(b"original still owned").unwrap();
        drop(file);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn duplicate_is_not_inherited_and_original_survives_drop() {
        let (mut read, mut write) = pipe();
        // SAFETY: the live File owns the handle; only its inheritance bit changes.
        assert_ne!(
            unsafe {
                SetHandleInformation(
                    read.as_raw_handle(),
                    HANDLE_FLAG_INHERIT,
                    HANDLE_FLAG_INHERIT,
                )
            },
            0
        );
        let mut duplicate = duplicate_pipe(read.as_raw_handle() as usize).unwrap();
        let mut flags = u32::MAX;
        // SAFETY: duplicate is live and flags points to writable storage.
        assert_ne!(
            unsafe { GetHandleInformation(duplicate.as_raw_handle(), &mut flags) },
            0
        );
        assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
        write.write_all(b"ab").unwrap();
        let mut byte = [0; 1];
        duplicate.read_exact(&mut byte).unwrap();
        assert_eq!(&byte, b"a");
        drop(duplicate);
        read.read_exact(&mut byte).unwrap();
        assert_eq!(&byte, b"b");
    }
}
