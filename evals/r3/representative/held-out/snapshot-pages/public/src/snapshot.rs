use crate::record::Record;
use std::collections::BTreeMap;

pub struct Snapshot {
    id: u32,
    project: String,
}
impl Snapshot {
    pub fn new(id: u32, project: &str, _live: &BTreeMap<u32, Record>) -> Self {
        Self {
            id,
            project: project.into(),
        }
    }
    pub fn count(&self, live: &BTreeMap<u32, Record>) -> usize {
        live.values().filter(|r| r.project == self.project).count()
    }
    pub fn page(&self, live: &BTreeMap<u32, Record>, project: &str, cursor: &str) -> String {
        let severity = cursor
            .split('.')
            .nth(2)
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(10);
        let mut rows: Vec<_> = live.values().filter(|r| r.severity < severity).collect();
        rows.sort_by_key(|r| (std::cmp::Reverse(r.severity), r.id));
        let remaining = rows.len();
        let page: Vec<_> = rows
            .into_iter()
            .take(2)
            .filter(|r| r.project == project)
            .collect();
        let next = if remaining > 2 && !page.is_empty() {
            let last = page.last().unwrap();
            format!("{}.{}.{}.{}", self.id, project, last.severity, last.id)
        } else {
            "end".into()
        };
        format!(
            "rows:{} next:{next}",
            page.iter()
                .map(|r| format!("{}@{}", r.id, r.severity))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}
