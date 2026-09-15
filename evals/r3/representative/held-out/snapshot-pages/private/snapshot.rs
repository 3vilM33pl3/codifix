use crate::record::Record;
use std::collections::BTreeMap;

pub struct Snapshot {
    id: u32,
    project: String,
    rows: Vec<Record>,
}
impl Snapshot {
    pub fn new(id: u32, project: &str, live: &BTreeMap<u32, Record>) -> Self {
        let mut rows: Vec<_> = live
            .values()
            .filter(|r| r.project == project)
            .cloned()
            .collect();
        rows.sort_by_key(|r| (std::cmp::Reverse(r.severity), r.id));
        Self {
            id,
            project: project.into(),
            rows,
        }
    }
    pub fn count(&self, _live: &BTreeMap<u32, Record>) -> usize {
        self.rows.len()
    }
    pub fn page(&self, _live: &BTreeMap<u32, Record>, project: &str, cursor: &str) -> String {
        if project != self.project {
            return "invalid-cursor".into();
        }
        let after = if cursor == "-" {
            None
        } else {
            let parts: Vec<_> = cursor.split('.').collect();
            if parts.len() != 4
                || parts[0].parse::<u32>().ok() != Some(self.id)
                || parts[1] != self.project
            {
                return "invalid-cursor".into();
            }
            let (Ok(severity), Ok(id)) = (parts[2].parse::<u8>(), parts[3].parse::<u32>()) else {
                return "invalid-cursor".into();
            };
            if !self
                .rows
                .iter()
                .any(|r| r.severity == severity && r.id == id)
            {
                return "invalid-cursor".into();
            }
            Some((severity, id))
        };
        let rows: Vec<_> = self
            .rows
            .iter()
            .filter(|r| {
                after.is_none_or(|(severity, id)| {
                    r.severity < severity || (r.severity == severity && r.id > id)
                })
            })
            .collect();
        let page: Vec<_> = rows.iter().take(2).collect();
        let next = if rows.len() > 2 {
            let last = page.last().unwrap();
            format!("{}.{}.{}.{}", self.id, self.project, last.severity, last.id)
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
