use std::collections::BTreeMap;
#[derive(Clone)]
pub struct Artifact {
    pub source_version: u64,
    pub dependencies: BTreeMap<String, u64>,
    pub generation: u64,
}
#[derive(Default, Clone)]
pub struct Cache {
    pub artifacts: BTreeMap<String, Artifact>,
    pub next_generation: u64,
}
