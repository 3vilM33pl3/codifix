// SPDX-License-Identifier: AGPL-3.0-or-later
//! Current-user Credential Manager only. Bounded chunks support private setup
//! journals larger than one WinCred blob. A single credential is the commit
//! pointer; interrupted generations remain encrypted and are reclaimed on retry.
use super::{CredentialStore, NativeKeychain};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ptr::{null, null_mut};
use uuid::Uuid;
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_NOT_FOUND, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0,
    },
    Security::Credentials::{
        CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW,
        CredDeleteW, CredFree, CredReadW, CredWriteW,
    },
    System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
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
struct Lock(HANDLE);
impl Lock {
    fn acquire(target: &str) -> Result<Self> {
        let name = wide(&format!("Global\\{}", target.replace('/', "-")));
        // SAFETY: null means the current-token default DACL; terminated name.
        let handle = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
        ensure!(!handle.is_null(), "Windows credential lock unavailable");
        // SAFETY: owned mutex handle, finite wait; abandoned means ownership acquired.
        let wait = unsafe { WaitForSingleObject(handle, 10_000) };
        if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
            unsafe {
                CloseHandle(handle);
            }
            bail!("Windows credential store is busy or inaccessible");
        }
        Ok(Self(handle))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        // SAFETY: this object owns the mutex until drop.
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
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
    ensure!(
        unsafe { CredWriteW(&credential, 0) } != 0,
        "Windows Credential Manager write failed; no plaintext fallback is available"
    );
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
fn state(target: &str) -> Result<State> {
    let Some(bytes) = read(target)? else {
        return Ok(State::default());
    };
    let result: State = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("Windows credential commit record is invalid"))?;
    ensure!(
        result.garbage.len() <= 2 && result.current.iter().chain(&result.garbage).all(valid),
        "Windows credential generation is invalid"
    );
    Ok(result)
}
fn save(target: &str, state: &State) -> Result<()> {
    write(target, &serde_json::to_vec(state)?)
}
fn cleanup(target: &str, state: &mut State) -> Result<()> {
    for version in &state.garbage {
        for index in 0..version.chunks {
            remove(&chunk(target, version, index))?;
        }
    }
    state.garbage.clear();
    Ok(())
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
        cleanup(&target, &mut state)?;
        save(&target, &state)
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
