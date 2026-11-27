//! Operations implemented by the specialist worker. Mandates and wire requests
//! continue to carry capability names as strings.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum Operation {
    Upgrade,
    IncidentReport,
    Inspect,
    SshHostInspect,
    SshHostSign,
    SshHostInstall,
    PlanPrepare,
    PlanApply,
    PlanInspect,
    LinearTrack,
    ModelDraft,
    ConversationTurn,
    OpenBaoRecover,
    Delegate,
}

impl Operation {
    pub(crate) fn parse(kind: &str) -> Option<Self> {
        Some(match kind {
            "incident_report" => Self::IncidentReport,
            "upgrade_candidate" => Self::Upgrade,
            "inspect" => Self::Inspect,
            "ssh_host_inspect" => Self::SshHostInspect,
            "ssh_host_sign" => Self::SshHostSign,
            "ssh_host_install" => Self::SshHostInstall,
            "plan_prepare" => Self::PlanPrepare,
            "plan_apply" => Self::PlanApply,
            "linear_track" => Self::LinearTrack,
            "plan_inspect" => Self::PlanInspect,
            "model_draft" => Self::ModelDraft,
            "conversation_turn" => Self::ConversationTurn,
            "openbao_recover" => Self::OpenBaoRecover,
            "delegate" => Self::Delegate,
            _ => return None,
        })
    }

    pub(crate) fn criterion(self) -> &'static str {
        match self {
            Self::IncidentReport => "incident_report_processed",
            Self::Upgrade => "upgrade_verified",
            Self::Inspect => "status_observed",
            Self::SshHostInspect => "served_host_certificate_observed",
            Self::SshHostSign => "approved_certificate_handed_off",
            Self::SshHostInstall => "fresh_ssh_login_verified",
            Self::PlanPrepare => "plan_valid",
            Self::PlanApply => "linear_readback_verified",
            Self::LinearTrack => "linear_issue_readback_verified",
            Self::PlanInspect => "linear_state_observed",
            Self::ModelDraft => "model_output_valid_json",
            Self::ConversationTurn => codifix_agency::conversation::ACCEPTANCE,
            Self::OpenBaoRecover => "openbao_unsealed",
            Self::Delegate => "delegated_result_reported",
        }
    }

    // A delegation can carry an observable request but is never itself a
    // disclosed remote operation. New operations require explicit review.
    pub(crate) fn permits_peer_observation(self) -> bool {
        matches!(
            self,
            Self::Inspect
                | Self::PlanPrepare
                | Self::PlanApply
                | Self::PlanInspect
                | Self::ModelDraft
        )
    }
}
