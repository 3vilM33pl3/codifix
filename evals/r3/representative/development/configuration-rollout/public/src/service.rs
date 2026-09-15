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
    pub fn commit(&mut self, _expected: u64, fail: bool) -> &'static str {
        self.live = crate::merge::merge(&self.base, &self.overlay);
        self.revision += 1;
        if !crate::schema::valid(&self.live) {
            return "invalid-config";
        }
        if fail {
            return "reload-failed";
        }
        "applied"
    }
}
