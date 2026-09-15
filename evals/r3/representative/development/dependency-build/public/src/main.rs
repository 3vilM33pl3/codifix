mod cache;
mod command;
mod graph;
mod planner;
use std::io::{self, BufRead};
fn main() {
    let mut graph = graph::Graph::default();
    let mut cache = cache::Cache::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        match command::parse(&line) {
            Some(command::Command::Node(name, version)) => {
                graph.node(&name, version);
                println!("staged");
            }
            Some(command::Command::Edge(child, parent, add)) => println!(
                "{}",
                if graph.edge(&child, &parent, add) {
                    "staged"
                } else {
                    "invalid"
                }
            ),
            Some(command::Command::Build(target, fail)) => {
                println!("{}", planner::build(&graph, &mut cache, &target, fail))
            }
            Some(command::Command::Show) => println!(
                "cache:{}",
                cache
                    .artifacts
                    .iter()
                    .map(|(n, a)| format!("{n}@{}", a.generation))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            None => println!("invalid"),
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn single_target_can_be_built_then_reused() {
        let mut graph = crate::graph::Graph::default();
        let mut cache = crate::cache::Cache::default();
        graph.node("app", 1);
        assert_eq!(
            crate::planner::build(&graph, &mut cache, "app", false),
            "built:app"
        );
        assert_eq!(
            crate::planner::build(&graph, &mut cache, "app", false),
            "cached"
        );
    }
    #[test]
    fn malformed_revision_and_unknown_edge_are_rejected() {
        assert!(crate::command::parse("node app 0").is_none());
        assert!(!crate::graph::Graph::default().edge("app", "unknown", true));
    }
}
