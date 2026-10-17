// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded candidate discovery is NOT session ownership. Only Restart Manager
//! resource observations matching a live PID's exact creation time are admitted.
//! This collector never calls RmShutdown/RmRestart or requests debug privileges.
use std::{
    fs,
    path::{Path, PathBuf},
};

const MAX_CANDIDATES: usize = 32;
const MAX_ENTRIES: usize = 4096;

fn candidates(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_owned()];
    let mut visited = 0;
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        if visited >= MAX_ENTRIES {
            break;
        }
        let Ok(metadata) = fs::symlink_metadata(&dir) else {
            continue;
        };
        if !plain(&metadata) || !metadata.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut directories = Vec::new();
        for entry in entries.flatten().take(MAX_ENTRIES - visited) {
            visited += 1;
            let Ok(meta) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if !plain(&meta) {
                continue;
            }
            if meta.is_dir() {
                directories.push(entry.path());
            } else if meta.is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|s| s.starts_with("rollout-") && s.ends_with(".jsonl"))
                && let Ok(modified) = meta.modified()
            {
                files.push((modified, entry.path()));
            }
        }
        // Prioritize recent year/month/day directories without following links.
        directories.sort();
        pending.extend(directories);
    }
    files.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    files
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|(_, path)| path)
        .collect()
}
fn plain(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    true
}

fn same_generation(expected: u64, observed: u64, current: u64) -> bool {
    expected != 0 && expected == observed && current == observed
}

fn generation_started_at(ticks: u64) -> Option<u64> {
    // Preserve shared snapshot resolution but never look up this PID again.
    ticks
        .checked_sub(116_444_736_000_000_000)
        .map(|unix| unix / 10_000_000 * 1000)
}

#[cfg(windows)]
pub(super) struct ObservedRollouts {
    handle: std::os::windows::io::OwnedHandle,
    pub started_at: u64,
    pub paths: Vec<PathBuf>,
}
#[cfg(windows)]
impl ObservedRollouts {
    pub fn is_live(&self) -> bool {
        use std::os::windows::io::AsRawHandle;
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(
                self.handle.as_raw_handle(),
                0,
            ) == windows_sys::Win32::Foundation::WAIT_TIMEOUT
        }
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        collections::HashMap,
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_MORE_DATA, FILETIME},
        System::{
            RestartManager::{
                RM_PROCESS_INFO, RmEndSession, RmGetList, RmRegisterResources, RmStartSession,
            },
            Threading::{
                GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_SYNCHRONIZE,
            },
        },
    };

    struct Session(u32);
    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                RmEndSession(self.0);
            }
        }
    }
    fn ticks(time: FILETIME) -> u64 {
        (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
    }
    fn generation(pid: u32) -> Option<(OwnedHandle, u64)> {
        let raw = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        };
        if raw.is_null() {
            return None;
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let (mut start, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        if unsafe {
            GetProcessTimes(
                handle.as_raw_handle(),
                &mut start,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        } == 0
        {
            return None;
        }
        Some((handle, ticks(start)))
    }
    fn users(path: &Path) -> Option<Vec<(u32, u64)>> {
        let mut id = 0;
        let mut key = [0u16; 33];
        if unsafe { RmStartSession(&mut id, 0, key.as_mut_ptr()) } != 0 {
            return None;
        }
        let session = Session(id);
        let path: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if path[..path.len() - 1].contains(&0) {
            return None;
        }
        let names = [path.as_ptr()];
        if unsafe { RmRegisterResources(session.0, 1, names.as_ptr(), 0, null(), 0, null()) } != 0 {
            return None;
        }
        let (mut needed, mut count, mut reasons) = (0, 0, 0);
        let code =
            unsafe { RmGetList(session.0, &mut needed, &mut count, null_mut(), &mut reasons) };
        if code == 0 {
            return Some(Vec::new());
        }
        if code != ERROR_MORE_DATA || needed > 256 {
            return None;
        }
        // One bounded retry only; changing resources remain unavailable.
        let mut entries = vec![RM_PROCESS_INFO::default(); needed as usize];
        count = needed;
        if unsafe {
            RmGetList(
                session.0,
                &mut needed,
                &mut count,
                entries.as_mut_ptr(),
                &mut reasons,
            )
        } != 0
        {
            return None;
        }
        entries.truncate(count as usize);
        Some(
            entries
                .into_iter()
                .map(|info| {
                    (
                        info.Process.dwProcessId,
                        ticks(info.Process.ProcessStartTime),
                    )
                })
                .collect(),
        )
    }
    pub(super) fn observed(pids: &[u32], root: &Path) -> HashMap<u32, ObservedRollouts> {
        let identities: HashMap<_, _> = pids
            .iter()
            .filter_map(|pid| Some((*pid, generation(*pid)?)))
            .collect();
        let mut mapped = HashMap::<u32, Vec<PathBuf>>::new();
        if identities.is_empty() {
            return HashMap::new();
        }
        for path in candidates(root) {
            let Some(users) = users(&path) else {
                continue;
            };
            for (pid, observed_start) in users {
                if identities.get(&pid).is_some_and(|(_, expected)| {
                    generation(pid).is_some_and(|(_, current)| {
                        same_generation(*expected, observed_start, current)
                    })
                }) {
                    mapped.entry(pid).or_default().push(path.clone());
                }
            }
        }
        for paths in mapped.values_mut() {
            paths.sort();
            paths.dedup();
        }
        identities
            .into_iter()
            .filter_map(|(pid, (handle, ticks))| {
                let observation = ObservedRollouts {
                    handle,
                    started_at: generation_started_at(ticks)?,
                    paths: mapped.remove(&pid)?,
                };
                observation.is_live().then_some((pid, observation))
            })
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn native_restart_manager_observes_multiple_open_files_not_closed_history() {
            let root = tempfile::tempdir().unwrap();
            let first = fs::File::create(root.path().join("rollout-first.jsonl")).unwrap();
            let second = fs::File::create(root.path().join("rollout-second.jsonl")).unwrap();
            fs::write(root.path().join("rollout-closed.jsonl"), b"history").unwrap();
            let pid = std::process::id();
            let mapped = observed(&[pid], root.path());
            assert_eq!(mapped.get(&pid).unwrap().paths.len(), 2);
            drop(second);
            assert_eq!(
                observed(&[pid], root.path()).get(&pid).unwrap().paths.len(),
                1
            );
            drop(first);
            assert!(observed(&[pid], root.path()).is_empty());
            assert!(observed(&[u32::MAX], root.path()).is_empty());
        }
    }
}
#[cfg(windows)]
pub(super) fn observed_rollouts(
    pids: &[u32],
    root: &Path,
) -> std::collections::HashMap<u32, ObservedRollouts> {
    native::observed(pids, root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_process_generations_are_not_resource_owners() {
        assert!(same_generation(12, 12, 12));
        assert!(!same_generation(12, 12, 13));
        assert!(!same_generation(12, 11, 12));
        assert!(!same_generation(0, 0, 0));
        let old = 116_444_736_000_000_000 + 12 * 10_000_000;
        let replacement = old + 10_000_000;
        assert!(!same_generation(old, old, replacement));
        assert_eq!(generation_started_at(old), Some(12_000));
        assert_eq!(generation_started_at(replacement), Some(13_000));
    }
    #[test]
    fn candidate_scan_is_bounded_and_never_follows_directory_links() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..(MAX_CANDIDATES + 5) {
            fs::write(root.path().join(format!("rollout-{index:04}.jsonl")), b"").unwrap();
        }
        fs::write(root.path().join("not-a-rollout.jsonl"), b"").unwrap();
        fs::create_dir(root.path().join("rollout-directory.jsonl")).unwrap();
        assert_eq!(candidates(root.path()).len(), MAX_CANDIDATES);
        #[cfg(unix)]
        {
            let external = tempfile::tempdir().unwrap();
            fs::write(external.path().join("rollout-hidden.jsonl"), b"").unwrap();
            std::os::unix::fs::symlink(external.path(), root.path().join("linked")).unwrap();
            assert!(
                !candidates(root.path())
                    .iter()
                    .any(|p| p.to_string_lossy().contains("hidden"))
            );
        }
    }
}
