// SPDX-License-Identifier: AGPL-3.0-or-later
//! Current-user Credential Manager only. Bounded chunks support private setup
//! journals larger than one WinCred blob. A single credential is the commit
//! pointer; interrupted generations remain encrypted and are reclaimed on retry.
use super::{CredentialStore, NativeKeychain};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ptr::null_mut;
use std::{collections::HashSet, ffi::c_void, mem::size_of, ptr::addr_of_mut};
use uuid::Uuid;
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_NO_TOKEN, ERROR_NOT_FOUND, GetLastError, HANDLE, LocalFree,
        WAIT_ABANDONED, WAIT_OBJECT_0,
    },
    Security::Credentials::{
        CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW,
        CredDeleteW, CredFree, CredReadW, CredWriteW,
    },
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER,
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SDDL_REVISION_1, SE_KERNEL_OBJECT,
        },
        DACL_SECURITY_INFORMATION, GetAce, GetTokenInformation, OWNER_SECURITY_INFORMATION,
        SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::Threading::{
        CreateMutexW, GetCurrentProcess, GetCurrentThread, MUTEX_ALL_ACCESS, OpenProcessToken,
        OpenThreadToken, ReleaseMutex, WaitForSingleObject,
    },
};
const BLOB: usize = CRED_MAX_CREDENTIAL_BLOB_SIZE as usize;
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Version {
    generation: Uuid,
    bytes: usize,
    chunks: usize,
    sha256: String,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    current: Option<Version>,
    garbage: Vec<Version>,
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}
fn target(service: &str, account: &str) -> Result<String> {
    ensure!(
        !service.contains('\0') && !account.contains('\0'),
        "invalid credential identifier"
    );
    let mut hash = Sha256::new();
    hash.update(service.as_bytes());
    hash.update([0]);
    hash.update(account.as_bytes());
    Ok(format!("MemoryLayer/v1/{:x}", hash.finalize()))
}
struct NativeHandle(HANDLE);
impl Drop for NativeHandle {
    fn drop(&mut self) {
        // SAFETY: every instance owns one successfully created native handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: the APIs used below allocate their outputs with LocalAlloc.
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn sid_string(sid: *mut c_void) -> Result<String> {
    ensure!(!sid.is_null(), "Windows credential lock owner unavailable");
    let mut value = null_mut();
    // SAFETY: the SID belongs to a live token or security descriptor.
    ensure!(
        unsafe { ConvertSidToStringSidW(sid, &mut value) } != 0,
        "Windows credential SID unavailable"
    );
    let _allocation = LocalAllocation(value.cast());
    let mut length = 0;
    // SAFETY: successful conversion supplies a NUL-terminated UTF-16 allocation.
    unsafe {
        while *value.add(length) != 0 {
            length += 1;
        }
        Ok(String::from_utf16(std::slice::from_raw_parts(
            value, length,
        ))?)
    }
}
fn current_user_sid() -> Result<String> {
    let mut raw = null_mut();
    // Respect an effective impersonation token; never scope a mutex to an email,
    // environment variable, display name, or unrelated process account.
    if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut raw) } == 0 {
        ensure!(
            unsafe { GetLastError() } == ERROR_NO_TOKEN,
            "Windows credential caller token unavailable"
        );
        ensure!(
            unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } != 0,
            "Windows credential process token unavailable"
        );
    }
    let token = NativeHandle(raw);
    let mut length = 0;
    // SAFETY: sizing call, then an aligned buffer large enough for TOKEN_USER.
    unsafe {
        GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut length);
    }
    ensure!(
        (size_of::<TOKEN_USER>() as u32..=65536).contains(&length),
        "Windows credential token exceeds bounds"
    );
    let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    ensure!(
        unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        } != 0,
        "Windows credential user SID unavailable"
    );
    // SAFETY: successful TokenUser query starts with TOKEN_USER, including its SID.
    sid_string(unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid })
}
fn mutex_name(target: &str, sid: &str) -> String {
    format!(
        "Global\\MemoryLayer-Credentials-v1-{sid}-{}",
        target.replace('/', "-")
    )
}
fn create_mutex(name: &str, sddl: &str) -> Result<NativeHandle> {
    let mut descriptor = null_mut();
    // SAFETY: NUL-terminated SDDL; returned descriptor owns all embedded pointers.
    ensure!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide(sddl).as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        } != 0,
        "Windows credential mutex policy unavailable"
    );
    let _allocation = LocalAllocation(descriptor);
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    // SAFETY: explicit owner/SYSTEM DACL, descriptor/name live through the call.
    let handle = unsafe { CreateMutexW(&attributes, 0, wide(name).as_ptr()) };
    ensure!(!handle.is_null(), "Windows credential lock unavailable");
    Ok(NativeHandle(handle))
}
fn verify_mutex(handle: HANDLE, sid: &str) -> Result<()> {
    let mut owner = null_mut();
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    // Existing named objects ignore CreateMutex's security descriptor. Verify
    // the returned object's actual policy before waiting on or trusting it.
    let result = unsafe {
        GetSecurityInfo(
            handle,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    ensure!(result == 0, "Windows credential mutex security unavailable");
    let _allocation = LocalAllocation(descriptor);
    ensure!(
        sid_string(owner)? == sid && !dacl.is_null(),
        "Windows credential lock has an untrusted owner or policy"
    );
    let mut owner_allowed = false;
    let mut system_allowed = false;
    // SAFETY: GetSecurityInfo returned a live ACL with a bounded ACE count.
    let count = unsafe { (*dacl).AceCount };
    for index in 0..u32::from(count) {
        let mut raw = null_mut();
        ensure!(
            unsafe { GetAce(dacl, index, &mut raw) } != 0,
            "Windows credential lock ACE unavailable"
        );
        // SAFETY: every returned ACE starts with ACE_HEADER.
        let header = unsafe { &*raw.cast::<ACE_HEADER>() };
        ensure!(
            header.AceType == 0
                && header.AceFlags == 0
                && usize::from(header.AceSize) >= size_of::<ACCESS_ALLOWED_ACE>(),
            "Windows credential lock has an unsupported policy"
        );
        // SAFETY: type and length validated; inline SID belongs to the live ACL.
        let ace = unsafe { &mut *raw.cast::<ACCESS_ALLOWED_ACE>() };
        let trustee = sid_string(addr_of_mut!(ace.SidStart).cast())?;
        ensure!(
            ace.Mask == MUTEX_ALL_ACCESS,
            "Windows credential mutex permission mismatch"
        );
        ensure!(
            trustee == sid || trustee == "S-1-5-18",
            "Windows credential lock permits another principal"
        );
        owner_allowed |= trustee == sid;
        system_allowed |= trustee == "S-1-5-18";
    }
    ensure!(
        owner_allowed && system_allowed,
        "Windows credential lock owner/SYSTEM grants are incomplete"
    );
    Ok(())
}
struct Lock(NativeHandle);
impl Lock {
    fn acquire(target: &str) -> Result<Self> {
        let sid = current_user_sid()?;
        let name = mutex_name(target, &sid);
        let handle = create_mutex(
            &name,
            &format!("O:{sid}G:{sid}D:P(A;;0x001F0001;;;{sid})(A;;0x001F0001;;;SY)"),
        )?;
        verify_mutex(handle.0, &sid)?;
        // SAFETY: owned mutex handle, finite wait; abandoned means ownership acquired.
        let wait = unsafe { WaitForSingleObject(handle.0, 10_000) };
        if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
            bail!("Windows credential store is busy or inaccessible");
        }
        Ok(Self(handle))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        // SAFETY: this object owns the mutex until drop.
        unsafe {
            ReleaseMutex(self.0.0);
        }
    }
}
struct Allocation(*mut CREDENTIALW);
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            CredFree(self.0.cast());
        }
    }
}
fn read(name: &str) -> Result<Option<Vec<u8>>> {
    let name = wide(name);
    let mut output = null_mut();
    // SAFETY: output pointer and terminated target live through the call.
    if unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut output) } == 0 {
        if unsafe { GetLastError() } == ERROR_NOT_FOUND {
            return Ok(None);
        }
        bail!("Windows Credential Manager read failed; no plaintext fallback is available");
    }
    ensure!(!output.is_null(), "Windows credential result unavailable");
    let allocation = Allocation(output);
    // SAFETY: successful CredReadW owns a complete CREDENTIALW allocation.
    let value = unsafe { &*allocation.0 };
    ensure!(
        value.Type == CRED_TYPE_GENERIC && value.CredentialBlobSize as usize <= BLOB,
        "Windows credential record exceeds its bound"
    );
    if value.CredentialBlobSize == 0 {
        return Ok(Some(Vec::new()));
    }
    ensure!(
        !value.CredentialBlob.is_null(),
        "Windows credential data unavailable"
    );
    Ok(Some(
        unsafe {
            std::slice::from_raw_parts(value.CredentialBlob, value.CredentialBlobSize as usize)
        }
        .to_vec(),
    ))
}
fn write(name: &str, bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() <= BLOB,
        "Windows credential chunk exceeds bound"
    );
    let mut name = wide(name);
    let mut user = wide("Memory Layer");
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: name.as_mut_ptr(),
        CredentialBlobSize: bytes.len() as u32,
        CredentialBlob: bytes.as_ptr().cast_mut(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: user.as_mut_ptr(),
        ..Default::default()
    };
    // SAFETY: all backing buffers outlive the call; API copies the input blob.
    if unsafe { CredWriteW(&credential, 0) } == 0 {
        // Capture immediately, without logging target names or credential bytes.
        let code = unsafe { GetLastError() };
        bail!(
            "Windows Credential Manager write failed (Windows error {code}); no plaintext fallback is available"
        );
    }
    Ok(())
}
fn remove(name: &str) -> Result<()> {
    let name = wide(name);
    if unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0
        && unsafe { GetLastError() } != ERROR_NOT_FOUND
    {
        bail!("Windows Credential Manager deletion failed; retry local cleanup");
    }
    Ok(())
}
fn chunk(target: &str, version: &Version, index: usize) -> String {
    format!("{target}/{}/{index}", version.generation.simple())
}
fn valid(version: &Version) -> bool {
    !version.generation.is_nil()
        && version.bytes <= MAX_BYTES
        && version.chunks == version.bytes.div_ceil(BLOB)
        && version.sha256.len() == 64
        && version.sha256.bytes().all(|b| b.is_ascii_hexdigit())
}
fn validate_state(state: &State) -> Result<()> {
    ensure!(
        state.garbage.len() <= 2 && state.current.iter().chain(&state.garbage).all(valid),
        "Windows credential generation is invalid"
    );
    let mut generations = HashSet::new();
    ensure!(
        state
            .current
            .iter()
            .chain(&state.garbage)
            .all(|version| generations.insert(version.generation)),
        "Windows credential commit and cleanup generations overlap or repeat"
    );
    Ok(())
}
fn state(target: &str) -> Result<State> {
    let Some(bytes) = read(target)? else {
        return Ok(State::default());
    };
    let result: State = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("Windows credential commit record is invalid"))?;
    validate_state(&result)?;
    Ok(result)
}
fn save(target: &str, state: &State) -> Result<()> {
    validate_state(state)?;
    write(target, &serde_json::to_vec(state)?)
}
fn cleanup(target: &str, state: &mut State) -> Result<()> {
    // Defense in depth: never delete current chunks, even if a future caller
    // constructs a malformed in-memory state rather than loading it via state().
    validate_state(state)?;
    for version in &state.garbage {
        for index in 0..version.chunks {
            remove(&chunk(target, version, index))?;
        }
    }
    state.garbage.clear();
    Ok(())
}
fn after_committed_cleanup(cleanup: impl FnOnce() -> Result<()>) {
    // The commit pointer already changed. A GC/outcome-record failure must not
    // be surfaced as a failed put, provoking callers to treat it as rollback.
    // The persisted previous garbage list remains safe and retryable.
    if cleanup().is_err() {
        eprintln!("Memory credentials committed; encrypted prior-generation cleanup is pending.");
    }
}
impl CredentialStore for NativeKeychain {
    fn get(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>> {
        let target = target(service, account)?;
        let _lock = Lock::acquire(&target)?;
        let state = state(&target)?;
        let Some(version) = state.current else {
            return Ok(None);
        };
        let mut bytes = Vec::with_capacity(version.bytes);
        for index in 0..version.chunks {
            let value = read(&chunk(&target, &version, index))?
                .ok_or_else(|| anyhow::anyhow!("Windows credential generation is incomplete"))?;
            ensure!(
                value.len() == BLOB.min(version.bytes - index * BLOB),
                "Windows credential chunk length mismatch"
            );
            bytes.extend(value);
        }
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == version.sha256,
            "Windows credential generation digest mismatch"
        );
        Ok(Some(bytes))
    }
    fn put(&self, service: &str, account: &str, value: &[u8]) -> Result<()> {
        ensure!(
            value.len() <= MAX_BYTES,
            "Windows credential value exceeds one MiB"
        );
        let target = target(service, account)?;
        let _lock = Lock::acquire(&target)?;
        let mut state = state(&target)?;
        cleanup(&target, &mut state)?;
        let next = Version {
            generation: Uuid::new_v4(),
            bytes: value.len(),
            chunks: value.len().div_ceil(BLOB),
            sha256: format!("{:x}", Sha256::digest(value)),
        };
        // Journal the generation before writing chunks; the current value remains visible.
        state.garbage.push(next.clone());
        save(&target, &state)?;
        for (index, bytes) in value.chunks(BLOB).enumerate() {
            write(&chunk(&target, &next, index), bytes)?;
        }
        state.garbage = state.current.take().into_iter().collect();
        state.current = Some(next);
        save(&target, &state)?;
        after_committed_cleanup(|| {
            cleanup(&target, &mut state)?;
            save(&target, &state)
        });
        Ok(())
    }
    fn delete(&self, service: &str, account: &str) -> Result<()> {
        let target = target(service, account)?;
        let _lock = Lock::acquire(&target)?;
        let mut state = state(&target)?;
        cleanup(&target, &mut state)?;
        if let Some(current) = state.current.take() {
            state.garbage.push(current);
        }
        save(&target, &state)?;
        cleanup(&target, &mut state)?;
        remove(&target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mutex_is_scoped_to_token_sid_and_rejects_an_existing_broad_dacl() {
        let sid = current_user_sid().expect("actual token SID");
        let target = target(
            &format!("memory-native-mutex-test-{}", Uuid::new_v4()),
            "one",
        )
        .expect("test target");
        assert_ne!(
            mutex_name(&target, &sid),
            mutex_name(&target, "S-1-5-19-other")
        );
        {
            let lock = Lock::acquire(&target).expect("private owner/SYSTEM lock");
            verify_mutex(lock.0.0, &sid).expect("actual mutex policy");
            assert!(verify_mutex(lock.0.0, "S-1-5-19-other").is_err());
        }
        let broad = create_mutex(
            &mutex_name(&target, &sid),
            &format!("O:{sid}G:{sid}D:P(A;;GA;;;WD)"),
        )
        .expect("synthetic hostile pre-created mutex");
        assert!(Lock::acquire(&target).is_err());
        drop(broad);
        let _private =
            Lock::acquire(&target).expect("untrusted mutex released, recreate private lock");
    }
    #[test]
    fn rejects_duplicate_or_current_cleanup_generation_before_deleting_chunks() {
        let service = format!("memory-native-state-test-{}", Uuid::new_v4());
        let store = NativeKeychain;
        store
            .put(&service, "one", b"accepted")
            .expect("initial value");
        let target = target(&service, "one").expect("test target");
        {
            let _lock = Lock::acquire(&target).expect("test lock");
            let mut original = state(&target).expect("original state");
            let current = original.current.clone().expect("current version");
            let mut malformed = State {
                current: Some(current.clone()),
                garbage: vec![current.clone()],
            };
            assert!(validate_state(&malformed).is_err());
            assert!(cleanup(&target, &mut malformed).is_err());
            assert_eq!(
                read(&chunk(&target, &current, 0)).expect("current chunk survived"),
                Some(b"accepted".to_vec())
            );
            let duplicate = State {
                current: None,
                garbage: vec![current.clone(), current.clone()],
            };
            assert!(validate_state(&duplicate).is_err());
            // Bypass save intentionally to simulate a malformed persisted record.
            write(
                &target,
                &serde_json::to_vec(&malformed).expect("malformed JSON"),
            )
            .expect("synthetic malformed commit");
            assert!(state(&target).is_err());
            original.garbage.clear();
            save(&target, &original).expect("restore original test commit");
        }
        assert_eq!(
            store.get(&service, "one").expect("preserved read"),
            Some(b"accepted".to_vec())
        );
        store.delete(&service, "one").expect("test cleanup");
    }
    #[test]
    fn post_commit_gc_error_does_not_report_a_rollback() {
        let called = std::cell::Cell::new(false);
        after_committed_cleanup(|| {
            called.set(true);
            bail!("synthetic GC failure")
        });
        assert!(called.get());
    }
    #[test]
    fn committed_new_value_stays_visible_with_pending_old_cleanup() {
        let service = format!("memory-native-gc-test-{}", Uuid::new_v4());
        let store = NativeKeychain;
        store.put(&service, "one", b"old").expect("initial value");
        let target = target(&service, "one").expect("target");
        {
            let _lock = Lock::acquire(&target).expect("test lock");
            let old = state(&target)
                .expect("old state")
                .current
                .expect("old version");
            let new = Version {
                generation: Uuid::new_v4(),
                bytes: 3,
                chunks: 1,
                sha256: format!("{:x}", Sha256::digest(b"new")),
            };
            write(&chunk(&target, &new, 0), b"new").expect("new chunks");
            save(
                &target,
                &State {
                    current: Some(new),
                    garbage: vec![old],
                },
            )
            .expect("new pointer committed before GC");
        }
        assert_eq!(
            store.get(&service, "one").expect("committed value"),
            Some(b"new".to_vec())
        );
        store
            .put(&service, "one", b"next")
            .expect("retry reclaims prior garbage");
        assert_eq!(
            store.get(&service, "one").expect("retry value"),
            Some(b"next".to_vec())
        );
        store.delete(&service, "one").expect("test cleanup");
    }
    #[test]
    fn interrupted_generation_keeps_previous_value_and_reclaims_on_retry() {
        let service = format!("memory-native-test-{}", Uuid::new_v4());
        let store = NativeKeychain;
        store
            .put(&service, "one", b"accepted")
            .expect("initial write");
        let target = target(&service, "one").expect("target");
        {
            let _lock = Lock::acquire(&target).expect("test lock");
            let mut state = state(&target).expect("test state");
            let pending = Version {
                generation: Uuid::new_v4(),
                bytes: BLOB + 1,
                chunks: 2,
                sha256: format!("{:x}", Sha256::digest(vec![42; BLOB + 1])),
            };
            state.garbage.push(pending.clone());
            save(&target, &state).expect("durable generation intent");
            write(&chunk(&target, &pending, 0), &vec![42; BLOB]).expect("first chunk");
        }
        assert_eq!(
            store.get(&service, "one").expect("interrupted read"),
            Some(b"accepted".to_vec())
        );
        store
            .put(&service, "one", b"retry")
            .expect("reconcile interrupted generation");
        assert_eq!(
            store.get(&service, "one").expect("retried read"),
            Some(b"retry".to_vec())
        );
        store.delete(&service, "one").expect("cleanup");
    }
    #[test]
    fn native_roundtrip_chunks_and_company_account_isolation() {
        let service = format!("memory-native-test-{}", Uuid::new_v4());
        let store = NativeKeychain;
        let value = vec![0x5a; 65_536];
        store
            .put(&service, "one", &value)
            .expect("native Credential Manager write");
        assert_eq!(
            store.get(&service, "one").expect("native read"),
            Some(value)
        );
        assert!(
            store
                .get(&service, "two")
                .expect("other account read")
                .is_none()
        );
        store
            .put(&service, "one", b"replacement")
            .expect("atomic replacement");
        assert_eq!(
            store.get(&service, "one").expect("replacement read"),
            Some(b"replacement".to_vec())
        );
        store.delete(&service, "one").expect("native cleanup");
        store.delete(&service, "one").expect("idempotent cleanup");
        assert!(store.get(&service, "one").expect("deleted read").is_none());
    }
}
