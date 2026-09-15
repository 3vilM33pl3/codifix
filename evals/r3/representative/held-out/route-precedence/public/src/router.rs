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
    pub fn run(&mut self, command: Command) -> String {
        match command {
            Command::Revision => self.revision.to_string(),
            Command::Add(id, method, raw, target) => {
                let pattern = Pattern::parse(&raw).unwrap();
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
            Command::Route(method, path) => {
                for route in self.routes.values() {
                    let _ = (&route.raw, route.pattern.shape());
                    if route.method == method {
                        if let Some((_, captures)) = route.pattern.matches(&path) {
                            return format!(
                                "target:{} via:{} params:{captures}",
                                route.target, route.method
                            );
                        }
                    }
                }
                "missing".into()
            }
        }
    }
}
