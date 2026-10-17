// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only SCM observation. Never creates, adopts, starts or stops services.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsString,
    mem::size_of,
    os::windows::ffi::OsStringExt,
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    System::{Com::CoTaskMemFree, Services::*},
    UI::Shell::{FOLDERID_ProgramData, FOLDERID_ProgramFiles, SHGetKnownFolderPath},
};

struct Sc(SC_HANDLE);
impl Drop for Sc {
    fn drop(&mut self) {
        unsafe {
            CloseServiceHandle(self.0);
        }
    }
}

pub struct Observation {
    pub binary_path: String,
    pub account: String,
    pub state: u32,
    pub start_type: u32,
}

/// Native known folders, not caller-controlled ProgramData/ProgramFiles env.
pub fn installation_roots() -> Result<(PathBuf, PathBuf)> {
    fn folder(id: &windows_sys::core::GUID) -> Result<PathBuf> {
        let mut raw = null_mut();
        // SAFETY: valid known-folder identifier and output storage.
        ensure!(
            unsafe { SHGetKnownFolderPath(id, 0, null_mut(), &mut raw) } >= 0 && !raw.is_null(),
            "Windows installation folder unavailable"
        );
        let value = unsafe {
            let mut length = 0;
            while length < 32768 && *raw.add(length) != 0 {
                length += 1;
            }
            let value = if length < 32768 {
                Some(PathBuf::from(OsString::from_wide(
                    std::slice::from_raw_parts(raw, length),
                )))
            } else {
                None
            };
            CoTaskMemFree(raw.cast());
            value
        }
        .context("Windows installation folder exceeds bounds")?;
        ensure!(
            value.is_absolute(),
            "Windows installation folder is not absolute"
        );
        Ok(value)
    }
    Ok((
        folder(&FOLDERID_ProgramData)?.join("MemoryLayer"),
        folder(&FOLDERID_ProgramFiles)?
            .join("Memory Layer")
            .join("releases"),
    ))
}

fn string_in_buffer(pointer: *const u16, buffer: &[usize]) -> Result<String> {
    let start = buffer.as_ptr() as usize;
    let address = pointer as usize;
    let end = start + std::mem::size_of_val(buffer);
    ensure!(
        address >= start && address < end && address.is_multiple_of(2),
        "SCM string outside response"
    );
    let remaining = (end - address) / 2;
    // SAFETY: response pointer alignment/range checked; bounded by live buffer.
    let units = unsafe { std::slice::from_raw_parts(pointer, remaining) };
    let length = units
        .iter()
        .position(|unit| *unit == 0)
        .context("unterminated SCM string")?;
    Ok(String::from_utf16(&units[..length])?)
}

pub fn observe(name: &str) -> Result<Observation> {
    ensure!(
        name.starts_with("MemoryLayer-")
            && name.len() <= 60
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "invalid Memory service name"
    );
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    // SAFETY: null selects local active SCM; only read-only rights requested.
    let manager = Sc(unsafe { OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT) });
    ensure!(!manager.0.is_null(), "cannot inspect local service manager");
    let service = Sc(unsafe {
        OpenServiceW(
            manager.0,
            wide.as_ptr(),
            SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS,
        )
    });
    ensure!(
        !service.0.is_null(),
        "selected named service is unavailable or access denied; no fallback attempted"
    );
    let mut needed = 0;
    unsafe {
        QueryServiceConfigW(service.0, null_mut(), 0, &mut needed);
    }
    ensure!(
        (size_of::<QUERY_SERVICE_CONFIGW>() as u32..=65536).contains(&needed),
        "invalid SCM configuration size"
    );
    let mut buffer = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    ensure!(
        unsafe { QueryServiceConfigW(service.0, buffer.as_mut_ptr().cast(), needed, &mut needed) }
            != 0,
        "cannot inspect selected service configuration"
    );
    // SAFETY: successfully filled, aligned QUERY_SERVICE_CONFIGW response.
    let config = unsafe { &*buffer.as_ptr().cast::<QUERY_SERVICE_CONFIGW>() };
    ensure!(
        config.dwServiceType == SERVICE_WIN32_OWN_PROCESS,
        "selected registration is not an independent process service"
    );
    let binary_path = string_in_buffer(config.lpBinaryPathName, &buffer)?;
    let account = string_in_buffer(config.lpServiceStartName, &buffer)?;
    let mut status: SERVICE_STATUS_PROCESS = unsafe { std::mem::zeroed() };
    ensure!(
        unsafe {
            QueryServiceStatusEx(
                service.0,
                SC_STATUS_PROCESS_INFO,
                (&mut status as *mut SERVICE_STATUS_PROCESS).cast(),
                size_of::<SERVICE_STATUS_PROCESS>() as u32,
                &mut needed,
            )
        } != 0,
        "cannot inspect selected service state"
    );
    Ok(Observation {
        binary_path,
        account,
        state: status.dwCurrentState,
        start_type: config.dwStartType,
    })
}
