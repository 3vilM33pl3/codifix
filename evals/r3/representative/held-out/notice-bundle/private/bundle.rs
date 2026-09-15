#[derive(Default)]
pub struct Bundle {
    generation: u64,
    content: Option<String>,
}
impl Bundle {
    pub fn build(&mut self, registry: &crate::registry::Registry) -> String {
        let mut texts = std::collections::BTreeSet::new();
        let mut notices = Vec::new();
        let mut offers = Vec::new();
        for id in &registry.selected {
            let Some(component) = registry.components.get(id) else {
                return "invalid-metadata".into();
            };
            let Some(license) = registry.licenses.get(&component.license) else {
                return "invalid-metadata".into();
            };
            texts.insert(license.text.clone());
            notices.push(format!("{id}={}", component.notice));
            if crate::policy::required_offer(license) {
                let Some(offer) = &component.offer else {
                    return "invalid-metadata".into();
                };
                offers.push(format!("{id}={offer}"));
            }
        }
        let content = format!(
            "licenses:{} attributions:{} offers:{}",
            texts.into_iter().collect::<Vec<_>>().join(","),
            notices.join(","),
            offers.join(",")
        );
        if self.content.as_ref() == Some(&content) {
            return format!("unchanged:{}", self.generation);
        }
        let Some(generation) = self.generation.checked_add(1) else {
            return "invalid-metadata".into();
        };
        self.generation = generation;
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
