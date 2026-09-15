use crate::{command::Command, pattern::Pattern};
use std::collections::BTreeMap;
struct Route {
    method: String,
    raw: String,
    pattern: Pattern,
    target: String,
}
#[derive(Default)]
pub struct Router {
    routes: BTreeMap<u32, Route>,
    revision: u64,
}
impl Router {
    fn select(&self, method: &str, path: &[String]) -> Option<String> {
        self.routes
            .values()
            .filter(|r| r.method == method)
            .filter_map(|route| {
                route.pattern.matches(path).map(|(priority, captures)| {
                    (
                        priority,
                        format!(
                            "target:{} via:{} params:{captures}",
                            route.target, route.method
                        ),
                    )
                })
            })
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, output)| output)
    }
    pub fn run(&mut self, command: Command) -> String {
        match command {
            Command::Revision => self.revision.to_string(),
            Command::Add(id, method, raw, target) => {
                let pattern = Pattern::parse(&raw).unwrap();
                if self
                    .routes
                    .get(&id)
                    .is_some_and(|r| r.method == method && r.raw == raw && r.target == target)
                {
                    return "unchanged".into();
                }
                if self.routes.iter().any(|(other, r)| {
                    *other != id && r.method == method && r.pattern.shape() == pattern.shape()
                }) {
                    return "conflict".into();
                }
                if !self.routes.contains_key(&id) && self.routes.len() >= 32 {
                    return "full".into();
                }
                self.routes.insert(
                    id,
                    Route {
                        method,
                        raw,
                        pattern,
                        target,
                    },
                );
                self.revision += 1;
                "ok".into()
            }
            Command::Remove(id) => {
                if self.routes.remove(&id).is_none() {
                    return "missing".into();
                }
                self.revision += 1;
                "ok".into()
            }
            Command::Route(method, path) => self
                .select(&method, &path)
                .or_else(|| {
                    if method == "HEAD" {
                        self.select("GET", &path)
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "missing".into()),
        }
    }
}
