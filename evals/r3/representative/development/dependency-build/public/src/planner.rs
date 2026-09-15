use crate::{
    cache::{Artifact, Cache},
    graph::Graph,
};
use std::collections::BTreeMap;
pub fn build(graph: &Graph, cache: &mut Cache, target: &str, fail: bool) -> String {
    let Some(node) = graph.nodes.get(target) else {
        return "invalid-graph".into();
    };
    let mut order: Vec<_> = node.dependencies.iter().cloned().collect();
    order.push(target.into());
    let mut built = vec![];
    for name in order {
        let Some(node) = graph.nodes.get(&name) else {
            return "invalid-graph".into();
        };
        if cache
            .artifacts
            .get(&name)
            .is_some_and(|a| a.source_version == node.version)
        {
            continue;
        }
        cache.next_generation += 1;
        cache.artifacts.insert(
            name.clone(),
            Artifact {
                source_version: node.version,
                dependencies: BTreeMap::new(),
                generation: cache.next_generation,
            },
        );
        built.push(name);
    }
    if fail && !built.is_empty() {
        return "failed".into();
    }
    if built.is_empty() {
        "cached".into()
    } else {
        format!("built:{}", built.join(","))
    }
}
