use crate::{
    command::{Action, Command},
    permit::Permit,
    policy,
};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Circuit {
    permits: BTreeMap<u64, Permit>,
    next: u64,
    failures: usize,
    until: Option<u64>,
}
impl Circuit {
    pub fn run(&mut self, command: Command) -> String {
        if self.until.is_some_and(|until| command.time >= until) {
            self.until = None;
        }
        match command.action {
            Action::State => self
                .until
                .map(|t| format!("open until:{t}"))
                .unwrap_or_else(|| format!("closed failures:{}", self.failures)),
            Action::Call => {
                if self.until.is_some() {
                    return "open".into();
                }
                if self.permits.len() >= policy::MAX_ACTIVE {
                    return "busy".into();
                }
                self.next += 1;
                self.permits.insert(
                    self.next,
                    Permit {
                        expires: command.time + policy::LEASE,
                        probe: false,
                    },
                );
                format!("grant:{}", self.next)
            }
            Action::Finish(id, success) => {
                let Some(permit) = self.permits.remove(&id) else {
                    return "stale".into();
                };
                let _ = (permit.expires, permit.probe);
                if !success {
                    self.failures += 1;
                }
                if self.failures >= policy::THRESHOLD {
                    self.until = Some(command.time + policy::COOLDOWN);
                    self.failures = 0;
                }
                "done".into()
            }
        }
    }
}
