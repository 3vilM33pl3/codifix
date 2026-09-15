use crate::{
    command::{Action, Command},
    permit::Permit,
    policy,
};
use std::collections::BTreeMap;
#[derive(Default)]
enum Mode {
    #[default]
    Closed,
    Open(u64),
    Half,
}
#[derive(Default)]
pub struct Circuit {
    permits: BTreeMap<u64, Permit>,
    next: u64,
    failures: usize,
    mode: Mode,
    time: u64,
}
impl Circuit {
    fn open(&mut self, until: u64) {
        self.mode = Mode::Open(until);
        self.failures = 0;
        self.permits.clear();
    }
    fn advance(&mut self, time: u64) -> bool {
        if time < self.time {
            return false;
        }
        if matches!(self.mode, Mode::Half) {
            let expired = self
                .permits
                .values()
                .find(|p| p.probe && p.expires <= time)
                .map(|p| p.expires);
            if let Some(deadline) = expired {
                self.open(deadline + policy::COOLDOWN);
            }
        }
        self.permits.retain(|_, p| p.expires > time);
        if matches!(self.mode, Mode::Open(until) if until <= time) {
            self.mode = Mode::Half;
        }
        self.time = time;
        true
    }
    pub fn run(&mut self, command: Command) -> String {
        if !self.advance(command.time) {
            return "time".into();
        }
        match command.action {
            Action::State => match self.mode {
                Mode::Closed => format!("closed failures:{}", self.failures),
                Mode::Open(until) => format!("open until:{until}"),
                Mode::Half => {
                    if self.permits.is_empty() {
                        "half-open idle".into()
                    } else {
                        "half-open busy".into()
                    }
                }
            },
            Action::Call => {
                if matches!(self.mode, Mode::Open(_)) {
                    return "open".into();
                }
                let probe = matches!(self.mode, Mode::Half);
                if self.permits.len() >= policy::MAX_ACTIVE || (probe && !self.permits.is_empty()) {
                    return "busy".into();
                }
                self.next += 1;
                self.permits.insert(
                    self.next,
                    Permit {
                        expires: command.time + policy::LEASE,
                        probe,
                    },
                );
                format!("grant:{}", self.next)
            }
            Action::Finish(id, success) => {
                let Some(permit) = self.permits.remove(&id) else {
                    return "stale".into();
                };
                if permit.probe {
                    if success {
                        self.mode = Mode::Closed;
                        self.failures = 0;
                        self.permits.clear();
                    } else {
                        self.open(command.time + policy::COOLDOWN);
                    }
                } else if success {
                    self.failures = 0;
                } else {
                    self.failures += 1;
                    if self.failures == policy::THRESHOLD {
                        self.open(command.time + policy::COOLDOWN);
                    }
                }
                "done".into()
            }
        }
    }
}
