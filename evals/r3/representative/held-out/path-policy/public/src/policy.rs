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
    pub fn check(&self, _operation: &str, input: &str) -> String {
        if self
            .grants
            .iter()
            .any(|(_, prefix)| input.starts_with(prefix))
        {
            format!("allow:{input}")
        } else {
            "deny".into()
        }
    }
}
