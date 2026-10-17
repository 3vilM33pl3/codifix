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
    /// Conservative text-token charge using the locked model price sheet. Cache
    /// discounts are not assumed. Non-token fees require a different approved
    /// contract; callers must not silently report them as zero.
    pub fn for_model(model: &super::Model, input_tokens: u64, output_tokens: u64) -> Result<Self> {
        Self::for_reported_tokens(model, input_tokens, output_tokens, 0)
    }

    /// Price confirmed text usage using explicitly locked rates. Input includes
    /// cached tokens and output includes reasoning tokens: neither is added twice.
    /// This calculation is not tokenization or authentication of a provider receipt.
    pub fn for_reported_tokens(
        model: &super::Model,
        input_tokens: u64,
        output_tokens: u64,
        cached_input_tokens: u64,
    ) -> Result<Self> {
        ensure!(
            input_tokens <= model.max_input_tokens && output_tokens <= model.max_output_tokens,
            "request exceeds locked model limits"
        );
        ensure!(
            cached_input_tokens <= input_tokens,
            "invalid cached input usage"
        );
        ensure!(
            model
                .cached_input_microusd_per_million
                .is_none_or(|price| price <= model.input_microusd_per_million),
            "cached input price exceeds reservation price"
        );
        let cached_price = if cached_input_tokens == 0 {
            0
        } else {
            model
                .cached_input_microusd_per_million
                .context("cached input price is unknown")?
        };
        input_tokens
            .checked_add(output_tokens)
            .context("token overflow")?;
        let input = u128::from(input_tokens - cached_input_tokens)
            * u128::from(model.input_microusd_per_million);
        let cached = u128::from(cached_input_tokens) * u128::from(cached_price);
        let output = u128::from(output_tokens) * u128::from(model.output_microusd_per_million);
        let cost = input
            .checked_add(cached)
            .and_then(|cost| cost.checked_add(output))
            .context("price overflow")?
            .div_ceil(1_000_000);
        Ok(Self {
            input_tokens,
            output_tokens,
            microusd: u64::try_from(cost).context("cost overflow")?,
        })
    }

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
        #[serde(default)]
        task_limits: Option<Limits>,
    },
    Reserved {
        id: String,
        payload_sha256: String,
        allowance: Allowance,
        #[serde(default)]
        task_id: Option<String>,
    },
    Settled {
        id: String,
        usage: Allowance,
        #[serde(default)]
        result_sha256: Option<String>,
    },
}

#[derive(Debug)]
struct Request {
    allowance: Allowance,
    settled: bool,
    task_id: Option<String>,
    payload_sha256: String,
    usage: Option<Allowance>,
    result_sha256: Option<String>,
}

pub(crate) struct SavedRequest<'a> {
    pub task_id: Option<&'a str>,
    pub payload_sha256: &'a str,
    pub allowance: Allowance,
    pub usage: Option<Allowance>,
    pub result_sha256: Option<&'a str>,
}

/// Holds an exclusive OS file lock for its lifetime. A process crash releases
/// that lock, but never refunds a reservation. Partial journals fail closed.
pub struct RequestBudget {
    file: File,
    manifest_sha256: String,
    limits: Limits,
    task_limits: Option<Limits>,
    ledger: Ledger,
    tasks: BTreeMap<String, Ledger>,
    requests: BTreeMap<String, Request>,
    poisoned: bool,
}

impl RequestBudget {
    #[cfg(test)]
    pub(super) fn journal_test_reader(&self) -> File {
        // Duplicate the owning file handle, not a second path-based open:
        // Windows mandatory byte-range locks reject the latter even in this
        // process. Production locking and the journal format are unchanged.
        self.file
            .try_clone()
            .expect("duplicate owned budget handle")
    }

    pub(crate) fn identity(&self) -> &str {
        &self.manifest_sha256
    }
    pub(crate) fn contract(&self) -> (&Limits, Option<&Limits>) {
        (&self.limits, self.task_limits.as_ref())
    }
    pub(crate) fn saved_request(&self, id: &str) -> Option<SavedRequest<'_>> {
        self.requests.get(id).map(|r| SavedRequest {
            task_id: r.task_id.as_deref(),
            payload_sha256: &r.payload_sha256,
            allowance: r.allowance,
            usage: r.usage,
            result_sha256: r.result_sha256.as_deref(),
        })
    }
    pub fn create(path: &Path, manifest_sha256: &str, limits: Limits) -> Result<Self> {
        Self::create_inner(path, manifest_sha256, limits, None)
    }

    /// A coding sequence is one task across all of its provider calls/steps.
    /// Aggregate token/request/spend limits are enforced in addition to the
    /// campaign ceiling. Wall-clock deadlines still belong to the dispatcher.
    pub fn create_scoped(
        path: &Path,
        manifest_sha256: &str,
        limits: Limits,
        task_limits: Limits,
    ) -> Result<Self> {
        Self::create_inner(path, manifest_sha256, limits, Some(task_limits))
    }

    fn create_inner(
        path: &Path,
        manifest_sha256: &str,
        limits: Limits,
        task_limits: Option<Limits>,
    ) -> Result<Self> {
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
            task_limits: task_limits.clone(),
        };
        let mut budget = Self {
            file,
            manifest_sha256: manifest_sha256.into(),
            limits,
            task_limits,
            ledger: Ledger::default(),
            tasks: BTreeMap::new(),
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
        Self::resume_inner(path, manifest_sha256, limits, None)
    }

    pub fn resume_scoped(
        path: &Path,
        manifest_sha256: &str,
        limits: Limits,
        task_limits: Limits,
    ) -> Result<Self> {
        Self::resume_inner(path, manifest_sha256, limits, Some(task_limits))
    }

    fn resume_inner(
        path: &Path,
        manifest_sha256: &str,
        limits: Limits,
        task_limits: Option<Limits>,
    ) -> Result<Self> {
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
                task_limits: saved_task_limits,
            } if saved == manifest_sha256
                && serde_json::to_value(&saved_limits)? == serde_json::to_value(&limits)?
                && serde_json::to_value(&saved_task_limits)?
                    == serde_json::to_value(&task_limits)? => {}
            _ => anyhow::bail!("budget campaign/configuration mismatch"),
        }
        let mut budget = Self {
            file,
            manifest_sha256: manifest_sha256.into(),
            limits,
            task_limits,
            ledger: Ledger::default(),
            tasks: BTreeMap::new(),
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

    pub fn task_ledger(&self, id: &str) -> Option<&Ledger> {
        self.tasks.get(id)
    }

    /// Metadata-only identities for explicit host recovery. Never a retry grant.
    pub fn request_ids(&self) -> Vec<&str> {
        self.requests.keys().map(String::as_str).collect()
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
        self.reserve_inner(id, payload_sha256, allowance, None)
    }

    pub fn reserve_for_task(
        &mut self,
        task_id: &str,
        id: &str,
        payload_sha256: &str,
        allowance: Allowance,
    ) -> Result<()> {
        self.reserve_inner(id, payload_sha256, allowance, Some(task_id.into()))
    }

    fn reserve_inner(
        &mut self,
        id: &str,
        payload_sha256: &str,
        allowance: Allowance,
        task_id: Option<String>,
    ) -> Result<()> {
        let event = Event::Reserved {
            id: id.into(),
            payload_sha256: payload_sha256.into(),
            allowance,
            task_id,
        };
        self.validate(&event)?;
        self.append(&event)?; // fsync before the caller is allowed to dispatch.
        self.apply(&event)
    }

    /// Call only with confirmed provider usage. A missing/invalid receipt leaves
    /// the full allowance reserved and blocks further dispatch, including restart.
    pub fn settle(&mut self, id: &str, usage: Allowance) -> Result<()> {
        self.settle_result(id, usage, None)
    }

    pub(crate) fn settle_result(
        &mut self,
        id: &str,
        usage: Allowance,
        result_sha256: Option<String>,
    ) -> Result<()> {
        let event = Event::Settled {
            id: id.into(),
            usage,
            result_sha256,
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

    pub fn dispatch_for_task<T>(
        &mut self,
        task_id: &str,
        id: &str,
        payload_sha256: &str,
        allowance: Allowance,
        provider: impl FnOnce() -> Result<(T, Allowance)>,
    ) -> Result<T> {
        self.reserve_for_task(task_id, id, payload_sha256, allowance)?;
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
                task_id,
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
                match (&self.task_limits, task_id) {
                    (Some(limits), Some(task_id)) => {
                        ensure!(super::identifier(task_id), "invalid task identity");
                        let previous = self.tasks.get(task_id);
                        let mut next = Ledger {
                            requests: previous.map_or(0, |l| l.requests),
                            tokens: previous.map_or(0, |l| l.tokens),
                            microusd: previous.map_or(0, |l| l.microusd),
                        };
                        next.reserve(limits, allowance.tokens()?, allowance.microusd)?;
                    }
                    (None, None) => {}
                    _ => anyhow::bail!("task-scoped journal requires a task on every reservation"),
                }
            }
            Event::Settled {
                id,
                usage,
                result_sha256,
            } => {
                ensure!(
                    result_sha256.as_ref().is_none_or(|d| valid_digest(d)),
                    "invalid result digest"
                );
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
            Event::Reserved {
                id,
                allowance,
                task_id,
                payload_sha256,
            } => {
                self.ledger
                    .reserve(&self.limits, allowance.tokens()?, allowance.microusd)?;
                if let (Some(task_id), Some(limits)) = (task_id, &self.task_limits) {
                    self.tasks.entry(task_id.clone()).or_default().reserve(
                        limits,
                        allowance.tokens()?,
                        allowance.microusd,
                    )?;
                }
                self.requests.insert(
                    id.clone(),
                    Request {
                        allowance: *allowance,
                        settled: false,
                        task_id: task_id.clone(),
                        payload_sha256: payload_sha256.clone(),
                        usage: None,
                        result_sha256: None,
                    },
                );
            }
            Event::Settled {
                id,
                usage,
                result_sha256,
            } => {
                let request = self
                    .requests
                    .get_mut(id)
                    .context("unknown request receipt")?;
                self.ledger.tokens -= request.allowance.tokens()? - usage.tokens()?;
                self.ledger.microusd -= request.allowance.microusd - usage.microusd;
                if let Some(task_id) = &request.task_id {
                    let task = self.tasks.get_mut(task_id).context("missing task ledger")?;
                    task.tokens -= request.allowance.tokens()? - usage.tokens()?;
                    task.microusd -= request.allowance.microusd - usage.microusd;
                }
                request.settled = true;
                request.usage = Some(*usage);
                request.result_sha256 = result_sha256.clone();
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
        let Some(path) = std::env::var_os("CODIFIX_R3_BUDGET_CRASH_PATH") else {
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
            .env("CODIFIX_R3_BUDGET_CRASH_PATH", fixture.journal())
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
    fn locked_prices_round_up_and_reject_excess_tokens_or_overflow() {
        let mut model = super::super::tests::fixture().model;
        model.input_microusd_per_million = 1;
        model.output_microusd_per_million = 2;
        assert_eq!(Allowance::for_model(&model, 1, 1).unwrap().microusd, 1);
        assert!(Allowance::for_model(&model, model.max_input_tokens + 1, 1).is_err());
        assert!(Allowance::for_model(&model, 1, model.max_output_tokens + 1).is_err());
        model.input_microusd_per_million = 1_000_000;
        model.output_microusd_per_million = 2_000_000;
        assert_eq!(Allowance::for_model(&model, 20, 10).unwrap().microusd, 40);
        model.max_input_tokens = u64::MAX;
        model.input_microusd_per_million = u64::MAX;
        assert!(Allowance::for_model(&model, u64::MAX, 1).is_err());
        assert!(Allowance::for_model(&model, u64::MAX, 0).is_err());
    }

    #[test]
    fn cache_discounts_require_locked_prices_and_never_increase_reservations() {
        let mut model = super::super::tests::fixture().model;
        model.input_microusd_per_million = 2_000_000;
        model.output_microusd_per_million = 4_000_000;
        assert!(Allowance::for_reported_tokens(&model, 20, 10, 5).is_err());
        model.cached_input_microusd_per_million = Some(500_000);
        let reservation = Allowance::for_model(&model, 20, 10).unwrap();
        assert_eq!(reservation.microusd, 80);
        let usage = Allowance::for_reported_tokens(&model, 20, 10, 5).unwrap();
        assert_eq!(usage.input_tokens, 20);
        assert_eq!(usage.output_tokens, 10);
        assert_eq!(usage.microusd, 73); // 30 + 2.5 + 40, rounded up once.
        assert!(usage.microusd <= reservation.microusd);
        assert!(Allowance::for_reported_tokens(&model, 20, 10, 21).is_err());
        model.cached_input_microusd_per_million = Some(0);
        assert_eq!(
            Allowance::for_reported_tokens(&model, 20, 10, 20)
                .unwrap()
                .microusd,
            40
        );
        model.cached_input_microusd_per_million = Some(2_000_001);
        assert!(Allowance::for_model(&model, 20, 10).is_err());
    }

    #[test]
    fn absent_cache_price_preserves_legacy_model_bytes() {
        let model = super::super::tests::fixture().model;
        let bytes = serde_json::to_vec(&model).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("cached_input"));
        let decoded: super::super::Model = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.cached_input_microusd_per_million, None);
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
    }

    #[test]
    fn scoped_requests_enforce_task_and_campaign_caps_before_each_provider_call() {
        for task_limits in [
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
            let mut budget = RequestBudget::create_scoped(
                &fixture.journal(),
                &identity(),
                limits(),
                task_limits.clone(),
            )
            .unwrap();
            assert!(
                budget
                    .reserve("unscoped-bypass", &identity(), allowance())
                    .is_err()
            );
            budget
                .dispatch_for_task("sequence-1", "step-1", &identity(), allowance(), || {
                    Ok(((), allowance()))
                })
                .unwrap();
            assert!(
                budget
                    .dispatch_for_task::<()>(
                        "sequence-1",
                        "step-2",
                        &identity(),
                        allowance(),
                        || panic!("task limit must prevent dispatch")
                    )
                    .is_err()
            );
            assert_eq!(budget.ledger().requests, 1);
            budget
                .dispatch_for_task("sequence-2", "step-3", &identity(), allowance(), || {
                    Ok(((), allowance()))
                })
                .unwrap();
            assert_eq!(budget.task_ledger("sequence-1").unwrap().requests, 1);
            assert_eq!(budget.task_ledger("sequence-2").unwrap().requests, 1);
            drop(budget);
            assert!(RequestBudget::resume(&fixture.journal(), &identity(), limits()).is_err());
            let mut budget = RequestBudget::resume_scoped(
                &fixture.journal(),
                &identity(),
                limits(),
                task_limits,
            )
            .unwrap();
            assert_eq!(budget.task_ledger("sequence-1").unwrap().tokens, 30);
            assert!(
                budget
                    .reserve_for_task("sequence-1", "restart-bypass", &identity(), allowance())
                    .is_err()
            );
            budget
                .dispatch_for_task("sequence-3", "step-4", &identity(), allowance(), || {
                    Ok(((), allowance()))
                })
                .unwrap();
            assert!(
                budget
                    .dispatch_for_task::<()>(
                        "sequence-4",
                        "step-5",
                        &identity(),
                        allowance(),
                        || panic!("campaign cap must prevent dispatch")
                    )
                    .is_err()
            );
        }
    }

    #[test]
    fn scoped_uncertainty_and_confirmed_refunds_survive_reopen_without_resetting_history() {
        let fixture = Fixture::new();
        let mut budget =
            RequestBudget::create_scoped(&fixture.journal(), &identity(), limits(), limits())
                .unwrap();
        let used = Allowance {
            input_tokens: 4,
            output_tokens: 2,
            microusd: 3,
        };
        budget
            .dispatch_for_task("sequence", "first", &identity(), allowance(), || {
                Ok(((), used))
            })
            .unwrap();
        assert!(
            budget
                .dispatch_for_task::<()>(
                    "sequence",
                    "second",
                    &identity(),
                    allowance(),
                    || anyhow::bail!("sensitive failure")
                )
                .is_err()
        );
        assert_eq!(budget.task_ledger("sequence").unwrap().tokens, 36);
        drop(budget);
        let mut budget =
            RequestBudget::resume_scoped(&fixture.journal(), &identity(), limits(), limits())
                .unwrap();
        assert_eq!(budget.task_ledger("sequence").unwrap().microusd, 18);
        assert_eq!(budget.uncertain_ids(), ["second"]);
        assert!(
            budget
                .reserve_for_task("other", "third", &identity(), allowance())
                .is_err()
        );
        budget.settle("second", used).unwrap();
        assert_eq!(budget.task_ledger("sequence").unwrap().tokens, 12);
        assert_eq!(budget.task_ledger("sequence").unwrap().requests, 2);
        assert!(
            budget
                .reserve_for_task("other", "first", &identity(), allowance())
                .is_err()
        );
        drop(budget);
        let changed = Limits {
            requests: 4,
            ..limits()
        };
        assert!(
            RequestBudget::resume_scoped(&fixture.journal(), &identity(), limits(), changed)
                .is_err()
        );
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
        let mut observer = budget.journal_test_reader();
        let output = budget
            .dispatch("one", &identity(), allowance(), || {
                observer.rewind().expect("rewind owned journal reader");
                let mut saved = String::new();
                observer
                    .read_to_string(&mut saved)
                    .expect("durable reservation visible");
                assert!(saved.contains("reserved"));
                assert!(!saved.contains("settled"));
                Ok(("fake answer", used))
            })
            .expect("fake dispatch");
        drop(observer);
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
