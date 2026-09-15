use crate::{command::Command, record::Record};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub struct Filter {
    records: BTreeMap<u32, Record>,
    grants: BTreeSet<(u32, String)>,
    generation: u64,
}
impl Filter {
    pub fn run(&mut self, command: Command) -> String {
        match command {
            Command::Generation => self.generation.to_string(),
            Command::Put(id, record) => {
                self.records.insert(id, record);
                self.grants.retain(|(key, _)| *key != id);
                self.generation += 1;
                "ok".into()
            }
            Command::Drop(id) => {
                if self.records.remove(&id).is_none() {
                    return "missing".into();
                }
                self.grants.retain(|(key, _)| *key != id);
                self.generation += 1;
                "ok".into()
            }
            Command::Grant(id, dest, allow) => {
                if !self.records.contains_key(&id) {
                    return "missing".into();
                }
                let changed = if allow {
                    self.grants.insert((id, dest))
                } else {
                    self.grants.remove(&(id, dest))
                };
                if !changed {
                    return "unchanged".into();
                }
                self.generation += 1;
                "ok".into()
            }
            Command::Export(id, dest) => {
                let Some(record) = self.records.get(&id) else {
                    return "missing".into();
                };
                if !matches!(dest.as_str(), "human" | "model")
                    || !self.grants.contains(&(id, dest.clone()))
                {
                    return "denied".into();
                }
                crate::policy::disclose(record, &dest).unwrap_or_else(String::from)
            }
        }
    }
}
