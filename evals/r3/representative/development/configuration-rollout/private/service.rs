use std::collections::BTreeMap;
pub struct Service {
    pub base: BTreeMap<String, String>,
    pub overlay: BTreeMap<String, Option<String>>,
    pub live: BTreeMap<String, String>,
    pub revision: u64,
}
impl Service {
    pub fn new() -> Self {
        let base = crate::schema::defaults();
        Self {
            live: base.clone(),
            base,
            overlay: BTreeMap::new(),
            revision: 0,
        }
    }
    pub fn commit(&mut self, expected: u64, fail: bool) -> &'static str {
        if expected != self.revision {
            return "conflict";
        }
        let candidate = crate::merge::merge(&self.base, &self.overlay);
        if !crate::schema::valid(&candidate) {
            return "invalid-config";
        }
        if fail {
            return "reload-failed";
        }
        let Some(next) = self.revision.checked_add(1) else {
            return "conflict";
        };
        self.live = candidate;
        self.revision = next;
        "applied"
    }
}
