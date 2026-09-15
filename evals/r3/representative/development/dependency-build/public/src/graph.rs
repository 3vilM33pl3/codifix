use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone)]
pub struct Node {
    pub version: u64,
    pub dependencies: BTreeSet<String>,
}
#[derive(Default)]
pub struct Graph {
    pub nodes: BTreeMap<String, Node>,
}
impl Graph {
    pub fn node(&mut self, name: &str, version: u64) {
        self.nodes
            .entry(name.into())
            .and_modify(|n| n.version = version)
            .or_insert(Node {
                version,
                dependencies: BTreeSet::new(),
            });
    }
    pub fn edge(&mut self, child: &str, parent: &str, add: bool) -> bool {
        if !self.nodes.contains_key(parent) {
            return false;
        }
        let Some(node) = self.nodes.get_mut(child) else {
            return false;
        };
        if add {
            node.dependencies.insert(parent.into());
        } else {
            node.dependencies.remove(parent);
        }
        true
    }
}
