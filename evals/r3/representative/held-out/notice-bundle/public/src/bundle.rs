#[derive(Default)]
pub struct Bundle {
    generation: u64,
    content: Option<String>,
}
impl Bundle {
    pub fn build(&mut self, registry: &crate::registry::Registry) -> String {
        let mut texts = std::collections::BTreeSet::new();
        let mut notices = std::collections::BTreeMap::new();
        for (id, component) in &registry.components {
            if let Some(license) = registry.licenses.get(&component.license) {
                texts.insert(license.text.clone());
                notices
                    .entry(component.notice.clone())
                    .or_insert(id.clone());
                let _needs_offer = crate::policy::required_offer(license);
            }
        }
        let content = format!(
            "licenses:{} attributions:{} offers:",
            texts.into_iter().collect::<Vec<_>>().join(","),
            notices
                .into_iter()
                .map(|(notice, id)| format!("{id}={notice}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        if self.content.as_ref() == Some(&content) {
            return format!("unchanged:{}", self.generation);
        }
        self.generation += 1;
        self.content = Some(content);
        format!("built:{}", self.generation)
    }
    pub fn show(&self) -> String {
        match &self.content {
            Some(content) => format!("bundle:{} {content}", self.generation),
            None => "bundle:none".into(),
        }
    }
}
