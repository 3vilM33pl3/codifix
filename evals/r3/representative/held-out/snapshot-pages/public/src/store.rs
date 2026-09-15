#[derive(Default)]
pub struct Store {
    pub live: std::collections::BTreeMap<u32, crate::record::Record>,
    snapshots: std::collections::BTreeMap<u32, crate::snapshot::Snapshot>,
    next: u32,
}
impl Store {
    pub fn begin(&mut self, project: &str) -> String {
        let Some(id) = self.next.checked_add(1) else {
            return "invalid".into();
        };
        let snapshot = crate::snapshot::Snapshot::new(id, project, &self.live);
        let count = snapshot.count(&self.live);
        self.snapshots.insert(id, snapshot);
        self.next = id;
        format!("snapshot:{id} rows:{count}")
    }
    pub fn page(&self, id: u32, project: &str, cursor: &str) -> String {
        match self.snapshots.get(&id) {
            Some(snapshot) => snapshot.page(&self.live, project, cursor),
            None => "missing-snapshot".into(),
        }
    }
    pub fn expire(&mut self, id: u32) -> String {
        if self.snapshots.remove(&id).is_some() {
            "expired".into()
        } else {
            "missing-snapshot".into()
        }
    }
}
