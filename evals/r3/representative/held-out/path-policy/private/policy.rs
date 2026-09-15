#[derive(Default)]
pub struct Policy {
    grants: std::collections::BTreeSet<(String, String)>,
    links: crate::links::Links,
}

impl Policy {
    pub fn grant(&mut self, operation: &str, input: &str) -> String {
        let Some(path) = crate::path::normalize(input).filter(|p| crate::path::under(p, "/work"))
        else {
            return "invalid".into();
        };
        self.grants.insert((operation.into(), path));
        "ok".into()
    }
    pub fn revoke(&mut self, operation: &str, input: &str) -> String {
        let Some(path) = crate::path::normalize(input).filter(|p| crate::path::under(p, "/work"))
        else {
            return "invalid".into();
        };
        self.grants.remove(&(operation.into(), path));
        "ok".into()
    }
    pub fn link(&mut self, input: &str, target: &str) -> String {
        let Some(source) = crate::path::normalize(input).filter(|p| crate::path::under(p, "/work"))
        else {
            return "invalid".into();
        };
        let Some(target) = crate::path::normalize(target) else {
            return "invalid".into();
        };
        self.links.targets.insert(source, target);
        "ok".into()
    }
    pub fn unlink(&mut self, input: &str) -> String {
        let Some(source) = crate::path::normalize(input).filter(|p| crate::path::under(p, "/work"))
        else {
            return "invalid".into();
        };
        self.links.targets.remove(&source);
        "ok".into()
    }
    fn permitted(&self, operation: &str, path: &str) -> bool {
        crate::path::under(path, "/work")
            && self
                .grants
                .iter()
                .any(|(op, prefix)| op == operation && crate::path::under(path, prefix))
    }

    pub fn check(&self, operation: &str, input: &str) -> String {
        let Some(requested) = crate::path::normalize(input) else {
            return "deny".into();
        };
        if !self.permitted(operation, &requested) {
            return "deny".into();
        }
        let mut current = requested;
        let mut seen = std::collections::BTreeSet::new();
        for rewrites in 0..=8 {
            if !seen.insert(current.clone()) {
                return "deny".into();
            }
            let selected = self
                .links
                .targets
                .iter()
                .filter(|(source, _)| crate::path::under(&current, source))
                .max_by_key(|(source, _)| source.len());
            let Some((source, target)) = selected else {
                return if self.permitted(operation, &current) {
                    format!("allow:{current}")
                } else {
                    "deny".into()
                };
            };
            if rewrites == 8 {
                return "deny".into();
            }
            let candidate = format!("{}{}", target, &current[source.len()..]);
            let Some(resolved) = crate::path::normalize(&candidate) else {
                return "deny".into();
            };
            current = resolved;
        }
        "deny".into()
    }
}
