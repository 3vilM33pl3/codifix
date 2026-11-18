// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::commands::api::TranscriptBatch;
use codifix_record::agency_observation::{ExchangeDetail, ExchangeSummary};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const HISTORY_RECHECK: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(super) struct LiveTranscript {
    exchanges: BTreeMap<Uuid, Arc<ExchangeDetail>>,
    archive_before: Option<String>,
    archive_seen: BTreeSet<Uuid>,
    last_archive_check: Option<Instant>,
    last_poll: Option<Instant>,
    paused: Option<BTreeMap<Uuid, Arc<ExchangeDetail>>>,
    pub(super) scroll: usize,
    pub(super) loaded: bool,
}
impl LiveTranscript {
    pub(super) fn due(&self) -> bool {
        self.last_poll
            .is_none_or(|at| at.elapsed() >= POLL_INTERVAL)
    }

    pub(super) fn begin_poll(&mut self) {
        self.last_poll = Some(Instant::now());
    }

    pub(super) fn archive_before(&self) -> Option<&str> {
        self.archive_before.as_deref()
    }

    pub(super) fn known(&self) -> BTreeMap<Uuid, ExchangeSummary> {
        self.exchanges
            .iter()
            .map(|(id, detail)| (*id, detail.exchange.clone()))
            .collect()
    }

    pub(super) fn accept(&mut self, latest: TranscriptBatch, archive: Option<TranscriptBatch>) {
        self.loaded = true;
        // Only discard cached messages once their entire authorized task list
        // has been checked. A task that moved off the latest page still exists.
        let complete = if latest.page.next_before.is_none() {
            self.archive_seen.clear();
            self.archive_before = None;
            self.archive_seen
                .extend(latest.page.exchanges.iter().map(|e| e.task_id));
            true
        } else if let Some(archive) = &archive {
            self.archive_seen
                .extend(latest.page.exchanges.iter().map(|e| e.task_id));
            self.archive_seen
                .extend(archive.page.exchanges.iter().map(|e| e.task_id));
            self.archive_before = archive.page.next_before.clone();
            self.archive_before.is_none()
        } else {
            if self.archive_before.is_none()
                && self
                    .last_archive_check
                    .is_none_or(|at| at.elapsed() >= HISTORY_RECHECK)
            {
                self.archive_seen.clear();
                self.archive_seen
                    .extend(latest.page.exchanges.iter().map(|e| e.task_id));
                self.archive_before = latest.page.next_before.clone();
            }
            false
        };
        for batch in archive.into_iter().chain([latest]) {
            for item in batch.details {
                let id = item.exchange.task_id;
                if self
                    .exchanges
                    .get(&id)
                    .is_none_or(|old| item.exchange.updated_at >= old.exchange.updated_at)
                {
                    self.exchanges.insert(id, Arc::new(item));
                }
            }
        }
        if complete {
            let retained = self.exchanges.len();
            self.exchanges
                .retain(|id, _| self.archive_seen.contains(id));
            if self.exchanges.len() != retained {
                self.resume();
            }
            self.last_archive_check = Some(Instant::now());
            self.archive_seen.clear();
        }
    }

    pub(super) fn visible(&self) -> impl Iterator<Item = &ExchangeDetail> {
        self.paused
            .as_ref()
            .unwrap_or(&self.exchanges)
            .values()
            .map(Arc::as_ref)
    }

    pub(super) fn paused(&self) -> bool {
        self.paused.is_some()
    }

    pub(super) fn pause(&mut self, end: usize) {
        if !self.paused() {
            self.scroll = end;
            self.paused = Some(self.exchanges.clone());
        }
    }

    pub(super) fn resume(&mut self) {
        self.paused = None;
        self.scroll = 0;
    }
}
