use std::collections::BTreeSet;
#[derive(Default)]
pub struct Policy {
    pub grants: BTreeSet<String>,
}
impl Policy {
    pub fn allows(&self, publisher: &str) -> bool {
        self.grants.contains(publisher)
    }
}
