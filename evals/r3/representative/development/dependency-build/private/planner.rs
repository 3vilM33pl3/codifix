use crate::{
    cache::{Artifact, Cache},
    graph::Graph,
};
use std::collections::{BTreeMap, BTreeSet};
fn visit(
    graph: &Graph,
    name: &str,
    active: &mut BTreeSet<String>,
    done: &mut BTreeSet<String>,
    order: &mut Vec<String>,
) -> bool {
    if done.contains(name) {
        return true;
    }
    if !active.insert(name.into()) {
        return false;
    }
    let Some(node) = graph.nodes.get(name) else {
        return false;
    };
    for dependency in &node.dependencies {
        if !visit(graph, dependency, active, done, order) {
            return false;
        }
    }
    active.remove(name);
    done.insert(name.into());
    order.push(name.into());
    true
}
pub fn build(graph: &Graph, cache: &mut Cache, target: &str, fail: bool) -> String {
    let mut order = vec![];
    if !visit(
        graph,
        target,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut order,
    ) {
        return "invalid-graph".into();
    }
    let mut candidate = cache.clone();
    let mut built = vec![];
    for name in order {
        let node = &graph.nodes[&name];
        let dependencies: BTreeMap<_, _> = node
            .dependencies
            .iter()
            .map(|d| (d.clone(), candidate.artifacts[d].generation))
            .collect();
        if candidate
            .artifacts
            .get(&name)
            .is_some_and(|a| a.source_version == node.version && a.dependencies == dependencies)
        {
            continue;
        }
        let Some(next) = candidate.next_generation.checked_add(1) else {
            return "failed".into();
        };
        candidate.next_generation = next;
        candidate.artifacts.insert(
            name.clone(),
            Artifact {
                source_version: node.version,
                dependencies,
                generation: next,
            },
        );
        built.push(name);
    }
    if built.is_empty() {
        return "cached".into();
    }
    if fail {
        return "failed".into();
    }
    *cache = candidate;
    format!("built:{}", built.join(","))
}
