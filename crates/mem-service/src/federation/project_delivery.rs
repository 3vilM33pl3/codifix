// SPDX-License-Identifier: AGPL-3.0-or-later
use super::ConfiguredInstance;
use mem_federation::publication::{InstanceId, SpaceId};
use mem_federation_runtime::InstanceSync;
use uuid::Uuid;

impl ConfiguredInstance {
    #[cfg(test)]
    pub(crate) fn synthetic_project_delivery(
        sync: InstanceSync,
        instance: InstanceId,
        space: SpaceId,
        authority: InstanceId,
        project: Uuid,
    ) -> Self {
        Self {
            sync,
            instance,
            space,
            authority,
            project,
            projects: Default::default(),
            expertise_browser: false,
            expertise_callers: vec![],
            expertise_output_destinations: vec![],
            expertise_model_destinations: vec![],
            expertise_model: None,
        }
    }
    pub(crate) fn project_delivery(
        &self,
        project: Uuid,
    ) -> Option<(InstanceSync, InstanceId, SpaceId)> {
        self.for_project(project)
            .map(|view| (view.sync, view.instance, view.space))
    }
}
