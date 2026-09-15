// SPDX-License-Identifier: AGPL-3.0-or-later
//! Durable reservation boundary for a trusted campaign dispatcher. This module
//! makes no provider calls and does not grant permission to perform paid runs.
use super::{Ledger, Limits, valid_digest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

const MAX_JOURNAL_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allowance {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub microusd: u64,
}
impl Allowance {
    fn tokens(self) -> Result<u64> {
        self.input_tokens
            .checked_add(self.output_tokens)
            .context("token overflow")
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
enum Event {
    Opened {
        version: u8,
        manifest_sha256: String,
        limits: Limits,
    },
    Reserved {
        id: String,
        payload_sha256: String,
        allowance: Allowance,
    },
    Settled {
        id: String,
        usage: Allowance,
    },
}

#[derive(Debug)]
struct Request {
    allowance: Allowance,
    settled: bool,
}

/// Holds an exclusive OS file lock for its lifetime. A process crash releases
/// that lock, but never refunds a reservation. Partial journals fail closed.
pub struct RequestBudget {
    file: File,
    limits: Limits,
    ledger: Ledger,
    requests: BTreeMap<String, Request>,
    poisoned: bool,
}

impl RequestBudget {
    pub fn create(path: &Path, manifest_sha256: &str, limits: Limits) -> Result<Self> {
        ensure!(valid_digest(manifest_sha256), "invalid campaign identity");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(path)
            .context("create private budget journal")?;
        file.try_lock()
            .map_err(|_| anyhow::anyhow!("budget journal is already owned"))?;
        let header = Event::Opened {
            version: 1,
            manifest_sha256: manifest_sha256.into(),
            limits: limits.clone(),
        };
        let mut budget = Self {
            file,
            limits,
            ledger: Ledger::default(),
            requests: BTreeMap::new(),
            poisoned: false,
        };
        budget.append(&header)?;
        // Make the new directory entry durable before any request can dispatch.
        #[cfg(unix)]
        File::open(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?
        .sync_all()?;
        Ok(budget)
    }

    pub fn resume(path: &Path, manifest_sha256: &str, limits: Limits) -> Result<Self> {
        ensure!(valid_digest(manifest_sha256), "invalid campaign identity");
        ensure!(
            std::fs::symlink_metadata(path)?.file_type().is_file(),
            "budget journal must be a regular file"
        );
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        file.try_lock()
            .map_err(|_| anyhow::anyhow!("budget journal is already owned"))?;
        ensure!(
            file.metadata()?.len() <= MAX_JOURNAL_BYTES,
            "oversized budget journal"
        );
        let mut bytes = Vec::new();
        (&mut file)
            .take(MAX_JOURNAL_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            !bytes.is_empty()
                && bytes.len() as u64 <= MAX_JOURNAL_BYTES
                && bytes.last() == Some(&b'\n'),
            "incomplete budget journal; reconciliation required"
        );
        let mut lines = bytes[..bytes.len() - 1].split(|b| *b == b'\n');
        let header: Event = serde_json::from_slice(lines.next().context("missing budget header")?)?;
        match header {
            Event::Opened {
                version: 1,
                manifest_sha256: saved,
                limits: saved_limits,
            } if saved == manifest_sha256
                && serde_json::to_value(&saved_limits)? == serde_json::to_value(&limits)? => {}
            _ => anyhow::bail!("budget campaign/configuration mismatch"),
        }
        let mut budget = Self {
            file,
            limits,
            ledger: Ledger::default(),
            requests: BTreeMap::new(),
            poisoned: false,
        };
        for line in lines {
            let event: Event = serde_json::from_slice(line).context("invalid budget journal")?;
            budget.apply(&event)?;
        }
        Ok(budget)
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// These requests may already have incurred their entire reserved charge.
    /// No automatic retry or new dispatch is permitted until reconciliation.
    pub fn uncertain_ids(&self) -> Vec<&str> {
        self.requests
            .iter()
            .filter(|(_, r)| !r.settled)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    pub fn reserve(&mut self, id: &str, payload_sha256: &str, allowance: Allowance) -> Result<()> {
        let event = Event::Reserved {
            id: id.into(),
            payload_sha256: payload_sha256.into(),
            allowance,
        };
        self.validate(&event)?;
        self.append(&event)?; // fsync before the caller is allowed to dispatch.
        self.apply(&event)
    }

    /// Call only with confirmed provider usage. A missing/invalid receipt leaves
    /// the full allowance reserved and blocks further dispatch, including restart.
    pub fn settle(&mut self, id: &str, usage: Allowance) -> Result<()> {
        let event = Event::Settled {
            id: id.into(),
            usage,
        };
        self.validate(&event)?;
        self.append(&event)?;
        self.apply(&event)
    }

    /// A synchronous dispatcher can use this boundary directly. An error or
    /// panic after reservation leaves an uncertain request, never a free retry.
    pub fn dispatch<T>(
        &mut self,
        id: &str,
        payload_sha256: &str,
        allowance: Allowance,
        provider: impl FnOnce() -> Result<(T, Allowance)>,
    ) -> Result<T> {
        self.reserve(id, payload_sha256, allowance)?;
        let (output, usage) = provider()
            .map_err(|_| anyhow::anyhow!("provider outcome uncertain; reconciliation required"))?;
        self.settle(id, usage)?;
        Ok(output)
    }

    fn validate(&self, event: &Event) -> Result<()> {
        ensure!(
            !self.poisoned,
            "budget persistence failed; reconciliation required"
        );
        match event {
            Event::Reserved {
                id,
                payload_sha256,
                allowance,
            } => {
                ensure!(
                    !id.is_empty()
                        && id.len() <= 128
                        && id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
                    "invalid request identity"
                );
                ensure!(valid_digest(payload_sha256), "invalid payload fingerprint");
                ensure!(
                    !self.requests.contains_key(id),
                    "request identity already used; do not redispatch"
                );
                ensure!(
                    self.uncertain_ids().is_empty(),
                    "unresolved request; reconciliation required"
                );
                ensure!(allowance.tokens()? > 0, "empty token allowance");
                let mut next = Ledger {
                    requests: self.ledger.requests,
                    tokens: self.ledger.tokens,
                    microusd: self.ledger.microusd,
                };
                next.reserve(&self.limits, allowance.tokens()?, allowance.microusd)?;
            }
            Event::Settled { id, usage } => {
                let request = self.requests.get(id).context("unknown request receipt")?;
                ensure!(!request.settled, "request receipt already recorded");
                ensure!(
                    usage.input_tokens <= request.allowance.input_tokens
                        && usage.output_tokens <= request.allowance.output_tokens
                        && usage.microusd <= request.allowance.microusd,
                    "usage exceeds reserved allowance"
                );
            }
            Event::Opened { .. } => anyhow::bail!("unexpected budget header"),
        }
        Ok(())
    }

    fn apply(&mut self, event: &Event) -> Result<()> {
        self.validate(event)?;
        match event {
            Event::Reserved { id, allowance, .. } => {
                self.ledger
                    .reserve(&self.limits, allowance.tokens()?, allowance.microusd)?;
                self.requests.insert(
                    id.clone(),
                    Request {
                        allowance: *allowance,
                        settled: false,
                    },
                );
            }
            Event::Settled { id, usage } => {
                let request = self
                    .requests
                    .get_mut(id)
                    .context("unknown request receipt")?;
                self.ledger.tokens -= request.allowance.tokens()? - usage.tokens()?;
                self.ledger.microusd -= request.allowance.microusd - usage.microusd;
                request.settled = true;
            }
            Event::Opened { .. } => unreachable!("validated event"),
        }
        Ok(())
    }

    fn append(&mut self, event: &Event) -> Result<()> {
        ensure!(
            !self.poisoned,
            "budget persistence failed; reconciliation required"
        );
        let mut line = serde_json::to_vec(event)?;
        line.push(b'\n');
        ensure!(
            self.file
                .metadata()?
                .len()
                .checked_add(line.len() as u64)
                .is_some_and(|n| n <= MAX_JOURNAL_BYTES),
            "budget journal capacity reached"
        );
        self.poisoned = true;
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&line)?;
        self.file.sync_all()?;
        self.poisoned = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_crash_child() {
        let Some(path) = std::env::var_os("MEMORY_R3_BUDGET_CRASH_PATH") else {
            return;
        };
        let mut budget =
            RequestBudget::create(Path::new(&path), &identity(), limits()).expect("child journal");
        budget
            .reserve("interrupted", &identity(), allowance())
            .expect("persist before termination");
        // Intentionally skips Rust destructors; no fake provider usage receipt.
        std::process::exit(17);
    }

    #[test]
    fn process_termination_releases_lock_but_retains_uncertain_charge() {
        let fixture = Fixture::new();
        let status = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", "campaign::budget::tests::budget_crash_child"])
            .env("MEMORY_R3_BUDGET_CRASH_PATH", fixture.journal())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("run terminating dispatcher");
        assert_eq!(status.code(), Some(17));
        let mut budget = RequestBudget::resume(&fixture.journal(), &identity(), limits())
            .expect("recover process journal");
        assert_eq!(budget.uncertain_ids(), ["interrupted"]);
        assert_eq!(budget.ledger().microusd, allowance().microusd);
        assert!(
            budget
                .reserve("replacement", &identity(), allowance())
                .is_err()
        );
    }

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("r3-budget-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&path).expect("private test directory");
            Self(path)
        }
        fn journal(&self) -> std::path::PathBuf {
            self.0.join("budget.jsonl")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn limits() -> Limits {
        Limits {
            requests: 3,
            tokens: 100,
            microusd: 50,
            task_seconds: 10,
        }
    }
    fn allowance() -> Allowance {
        Allowance {
            input_tokens: 20,
            output_tokens: 10,
            microusd: 15,
        }
    }
    fn identity() -> String {
        "a".repeat(64)
    }

    #[test]
    fn reservations_precede_dispatch_and_confirmed_usage_releases_only_unused_budget() {
        let fixture = Fixture::new();
        let path = fixture.journal();
        let mut budget =
            RequestBudget::create(&path, &identity(), limits()).expect("create journal");
        let used = Allowance {
            input_tokens: 5,
            output_tokens: 3,
            microusd: 4,
        };
        let output = budget
            .dispatch("one", &identity(), allowance(), || {
                let saved = std::fs::read_to_string(&path).expect("durable reservation visible");
                assert!(saved.contains("reserved"));
                assert!(!saved.contains("settled"));
                Ok(("fake answer", used))
            })
            .expect("fake dispatch");
        assert_eq!(output, "fake answer");
        assert_eq!(budget.ledger().tokens, 8);
        assert_eq!(budget.ledger().microusd, 4);
        assert_eq!(budget.ledger().requests, 1);
        assert!(budget.reserve("one", &identity(), allowance()).is_err());
        drop(budget);
        let recovered =
            RequestBudget::resume(&path, &identity(), limits()).expect("resume completed budget");
        assert_eq!(recovered.ledger().tokens, 8);
        assert!(recovered.uncertain_ids().is_empty());
    }

    #[test]
    fn uncertain_restart_blocks_new_calls_and_invalid_usage_cannot_refund() {
        let fixture = Fixture::new();
        let path = fixture.journal();
        let mut budget =
            RequestBudget::create(&path, &identity(), limits()).expect("create journal");
        let error = budget
            .dispatch::<()>("one", &identity(), allowance(), || {
                anyhow::bail!("SECRET_PROVIDER_BODY")
            })
            .expect_err("ambiguous provider");
        assert!(!error.to_string().contains("SECRET"));
        drop(budget);
        let mut budget =
            RequestBudget::resume(&path, &identity(), limits()).expect("recover pending request");
        assert_eq!(budget.uncertain_ids(), ["one"]);
        assert_eq!(budget.ledger().tokens, 30);
        assert!(
            budget
                .dispatch::<()>("two", &identity(), allowance(), || panic!(
                    "must not invoke provider"
                ))
                .is_err()
        );
        assert!(
            budget
                .settle(
                    "one",
                    Allowance {
                        input_tokens: 31,
                        output_tokens: 0,
                        microusd: 1
                    }
                )
                .is_err()
        );
        assert_eq!(budget.ledger().tokens, 30);
        budget
            .settle("one", allowance())
            .expect("explicit confirmed usage");
        assert!(budget.settle("one", allowance()).is_err());
    }

    #[test]
    fn campaign_caps_are_checked_before_each_fake_call() {
        for limited in [
            Limits {
                requests: 1,
                ..limits()
            },
            Limits {
                tokens: 35,
                ..limits()
            },
            Limits {
                microusd: 20,
                ..limits()
            },
        ] {
            let fixture = Fixture::new();
            let mut budget = RequestBudget::create(&fixture.journal(), &identity(), limited)
                .expect("create journal");
            budget
                .dispatch("one", &identity(), allowance(), || Ok(((), allowance())))
                .expect("first call");
            assert!(
                budget
                    .dispatch::<()>("two", &identity(), allowance(), || panic!(
                        "budget must prevent call"
                    ))
                    .is_err()
            );
            assert_eq!(budget.ledger().requests, 1);
        }
    }

    #[test]
    fn ownership_partial_writes_and_changed_campaigns_fail_closed() {
        let fixture = Fixture::new();
        let path = fixture.journal();
        let budget = RequestBudget::create(&path, &identity(), limits()).expect("create journal");
        assert!(RequestBudget::resume(&path, &identity(), limits()).is_err());
        assert!(RequestBudget::create(&path, &identity(), limits()).is_err());
        drop(budget);
        assert!(RequestBudget::resume(&path, &"b".repeat(64), limits()).is_err());
        assert!(
            RequestBudget::resume(
                &path,
                &identity(),
                Limits {
                    requests: 99,
                    ..limits()
                }
            )
            .is_err()
        );
        let mut file = OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("simulate torn write");
        file.write_all(b"{\"event\":")
            .expect("partial journal write");
        file.sync_all().expect("persist torn write");
        assert!(RequestBudget::resume(&path, &identity(), limits()).is_err());
    }
}
