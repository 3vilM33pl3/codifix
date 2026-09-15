#[derive(Default)]
pub struct Catalog {
    entries: std::collections::BTreeMap<u32, crate::record::Entry>,
}
impl Catalog {
    pub fn add(&mut self, id: u32, label: &str) -> String {
        let Some(key) = crate::keys::normalize(label) else {
            return "invalid".into();
        };
        if let Some(entry) = self.entries.get(&id) {
            return if entry.label == label {
                "unchanged".into()
            } else {
                "conflict-id".into()
            };
        }
        if let Some((owner, _)) = self.entries.iter().find(|(_, entry)| entry.key == key) {
            return format!("collision:{owner}");
        }
        self.entries.insert(
            id,
            crate::record::Entry {
                label: label.into(),
                key,
            },
        );
        "inserted".into()
    }
    pub fn get(&self, label: &str) -> String {
        let Some(key) = crate::keys::normalize(label) else {
            return "invalid".into();
        };
        match self.entries.iter().find(|(_, entry)| entry.key == key) {
            Some((id, entry)) => format!("found:{id}:{}", entry.label),
            None => "missing".into(),
        }
    }
    pub fn rename(&mut self, id: u32, label: &str) -> String {
        let Some(key) = crate::keys::normalize(label) else {
            return "invalid".into();
        };
        let Some(entry) = self.entries.get(&id) else {
            return "missing".into();
        };
        if entry.label == label {
            return "unchanged".into();
        }
        if let Some((owner, _)) = self
            .entries
            .iter()
            .find(|(owner, entry)| **owner != id && entry.key == key)
        {
            return format!("collision:{owner}");
        }
        self.entries.insert(
            id,
            crate::record::Entry {
                label: label.into(),
                key,
            },
        );
        "renamed".into()
    }
    pub fn remove(&mut self, id: u32) -> String {
        if self.entries.remove(&id).is_some() {
            "removed".into()
        } else {
            "missing".into()
        }
    }
    pub fn show(&self) -> String {
        format!(
            "entries:{}",
            self.entries
                .iter()
                .map(|(id, e)| format!("{id}={}@{}", e.key, e.label))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}
