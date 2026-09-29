// SPDX-License-Identifier: AGPL-3.0-or-later
//! Project-local policy behind one enrolled instance and one ordered receiver.
use super::{
    ConfiguredInstance, ExpertiseCaller, expertise, local_output, validate_expertise_callers,
};
use serde::Deserialize;
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectBinding {
    pub project: Uuid,
    pub principal: Uuid,
    #[serde(default)]
    pub expertise_principal: Option<Uuid>,
    #[serde(default)]
    pub expertise_browser: bool,
    #[serde(default)]
    pub expertise_callers: Vec<ExpertiseCaller>,
    #[serde(default = "local_output")]
    pub expertise_output_destinations: Vec<String>,
    #[serde(default)]
    pub expertise_model_destinations: Vec<String>,
    #[serde(default)]
    pub expertise_model: Option<expertise::ModelConfig>,
}

impl ProjectBinding {
    pub fn expertise_enabled(&self) -> bool {
        self.expertise_browser
            || self.expertise_principal.is_some()
            || !self.expertise_callers.is_empty()
    }

    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.project.is_nil() && !self.principal.is_nil(),
            "invalid federation project binding"
        );
        anyhow::ensure!(
            self.expertise_principal
                .is_none_or(|p| !p.is_nil() && p != self.principal),
            "expertise requires a separate project-scoped service identity"
        );
        validate_expertise_callers(&self.expertise_callers)?;
        for (values, required) in [
            (&self.expertise_output_destinations, true),
            (&self.expertise_model_destinations, false),
        ] {
            anyhow::ensure!(
                (!required || !values.is_empty())
                    && values.len() <= 16
                    && values
                        .iter()
                        .all(|d| mem_federation::expertise::valid_destination(d))
                    && values.iter().collect::<BTreeSet<_>>().len() == values.len(),
                "invalid project expertise destination policy"
            );
        }
        if let Some(model) = &self.expertise_model {
            model.validate()?;
        }
        Ok(())
    }
}

pub(super) fn validate_bindings(
    primary: ProjectBinding,
    additional: Vec<ProjectBinding>,
) -> anyhow::Result<Vec<ProjectBinding>> {
    anyhow::ensure!(
        additional.len() < 256,
        "too many federation project bindings"
    );
    let bindings: Vec<_> = std::iter::once(primary).chain(additional).collect();
    let mut projects = BTreeSet::new();
    let mut answerers = BTreeSet::new();
    for binding in &bindings {
        binding.validate()?;
        anyhow::ensure!(
            projects.insert(binding.project),
            "duplicate federation project binding"
        );
        if let Some(principal) = binding.expertise_principal {
            anyhow::ensure!(
                answerers.insert(principal),
                "each project requires its own expertise answerer"
            );
        }
    }
    Ok(bindings)
}

impl ConfiguredInstance {
    /// Returning a project view never borrows another project's caller policy.
    pub(super) fn for_project(&self, project: Uuid) -> Option<Self> {
        if self.project == project {
            return Some(self.clone());
        }
        let binding = self.projects.get(&project)?;
        let mut view = self.clone();
        view.project = project;
        view.expertise_browser = binding.expertise_browser;
        view.expertise_callers = binding.expertise_callers.clone();
        view.expertise_output_destinations = binding.expertise_output_destinations.clone();
        view.expertise_model_destinations = binding.expertise_model_destinations.clone();
        view.expertise_model = binding.expertise_model.clone();
        view.projects.clear();
        Some(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> ProjectBinding {
        serde_json::from_value(
            serde_json::json!({"project": Uuid::new_v4(), "principal": Uuid::new_v4()}),
        )
        .unwrap()
    }
    #[test]
    fn projects_have_independent_defaults_and_unique_answerers() {
        let mut a = binding();
        a.expertise_browser = true;
        a.expertise_output_destinations.push("openai".into());
        let mut b = binding();
        let both = validate_bindings(a.clone(), vec![b.clone()]).unwrap();
        assert!(!both[1].expertise_enabled());
        assert_eq!(both[1].expertise_output_destinations, vec!["local_human"]);
        assert!(validate_bindings(a.clone(), vec![a.clone()]).is_err());
        a.expertise_principal = Some(Uuid::new_v4());
        b.expertise_principal = a.expertise_principal;
        assert!(validate_bindings(a, vec![b]).is_err());
    }
    #[test]
    fn malformed_policy_is_rejected_before_runtime_registration() {
        let mut b = binding();
        b.expertise_output_destinations = vec!["https://untrusted.invalid".into()];
        assert!(validate_bindings(b, vec![]).is_err());
        let mut b = binding();
        b.project = Uuid::nil();
        assert!(validate_bindings(b, vec![]).is_err());
    }
}
