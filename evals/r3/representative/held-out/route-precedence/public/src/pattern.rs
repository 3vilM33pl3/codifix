use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone)]
enum Segment {
    Literal(String),
    Param(String),
    Rest(String),
}
#[derive(Clone)]
pub struct Pattern {
    segments: Vec<Segment>,
}
impl Pattern {
    pub fn parse(value: &str) -> Option<Self> {
        if value == "/" {
            return Some(Self {
                segments: Vec::new(),
            });
        }
        let parts = value.strip_prefix('/')?.split('/').collect::<Vec<_>>();
        if parts.len() > 8 {
            return None;
        }
        let mut names = BTreeSet::new();
        let mut segments = Vec::new();
        for (i, part) in parts.iter().enumerate() {
            if let Some(name) = part.strip_prefix(':') {
                if !crate::request::name(name) || !names.insert(name) {
                    return None;
                }
                segments.push(Segment::Param(name.into()));
            } else if let Some(name) = part.strip_prefix('*') {
                if i + 1 != parts.len() || !crate::request::name(name) || !names.insert(name) {
                    return None;
                }
                segments.push(Segment::Rest(name.into()));
            } else {
                if !crate::request::literal(part) {
                    return None;
                }
                segments.push(Segment::Literal((*part).into()));
            }
        }
        Some(Self { segments })
    }
    pub fn shape(&self) -> String {
        self.segments
            .iter()
            .map(|s| match s {
                Segment::Literal(v) => v.as_str(),
                Segment::Param(_) => ":",
                Segment::Rest(_) => "*",
            })
            .collect::<Vec<_>>()
            .join("/")
    }
    pub fn matches(&self, path: &[String]) -> Option<(Vec<u8>, String)> {
        let mut captures = BTreeMap::new();
        let mut priority = Vec::new();
        let mut consumed = 0;
        for segment in &self.segments {
            match segment {
                Segment::Literal(value) => {
                    if path.get(consumed)? != value {
                        return None;
                    }
                    consumed += 1;
                    priority.push(3);
                }
                Segment::Param(name) => {
                    captures.insert(name.clone(), path.get(consumed)?.clone());
                    consumed += 1;
                    priority.push(2);
                }
                Segment::Rest(name) => {
                    captures.insert(name.clone(), path[consumed..].join("/"));
                    consumed = path.len();
                    priority.push(1);
                }
            }
        }
        if consumed != path.len() {
            return None;
        }
        priority.push(4);
        let text = if captures.is_empty() {
            "-".into()
        } else {
            captures
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        Some((priority, text))
    }
}
