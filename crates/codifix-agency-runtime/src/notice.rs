//! Structured private status updates; the receiving service selects its Inbox.
use crate::{
    config::Config,
    social_transport::{Call, Transport},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeEvent {
    UnsealStarted,
    ContributionRequested,
    AwaitingOperator,
    ContributionAccepted,
    ContributionDeclined,
    InvalidShare,
    UnsealBlocked,
    UnsealedReady,
}

impl NoticeEvent {
    pub fn text(self) -> &'static str {
        match self {
            Self::UnsealStarted => "OpenBao is sealed. Unseal requested",
            Self::ContributionRequested => "Unseal contribution requested",
            Self::AwaitingOperator => "Waiting for the operator's unseal contribution",
            Self::ContributionAccepted => "Unseal contribution accepted",
            Self::ContributionDeclined => "Unseal contribution declined",
            Self::InvalidShare => "Unseal share rejected",
            Self::UnsealBlocked => "OpenBao unseal blocked",
            Self::UnsealedReady => "OpenBao unsealed and ready",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioNotice {
    pub task_id: Uuid,
    pub attempt: Uuid,
    pub subject: String,
    pub event: NoticeEvent,
    pub participant: Option<String>,
}

impl ScenarioNotice {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.task_id.is_nil() && !self.attempt.is_nil(),
            "notice identity required"
        );
        for name in std::iter::once(&self.subject).chain(&self.participant) {
            ensure!(
                !name.is_empty()
                    && name.len() <= 128
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)),
                "invalid enrolled notice label"
            );
        }
        Ok(())
    }

    pub async fn send(&self, store: &Store, cfg: &Config, peer_id: &str) -> Result<()> {
        self.validate()?;
        ensure!(
            cfg.social
                .as_ref()
                .is_some_and(|social| social.peers.contains_key(peer_id)),
            "enrolled AT recipient required"
        );
        let peer = cfg
            .peers
            .iter()
            .find(|p| p.id == peer_id)
            .context("unknown AT recipient")?;
        let message = serde_json::to_value(self)?;
        Transport::new(store, cfg, peer)?
            .call(Call::Notice(&message))
            .await?
            .error_for_status()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IncidentNotice {
    pub incident_id: Uuid,
    pub event_id: i64,
    pub service: String,
    pub state: String,
    pub summary: String,
}

impl IncidentNotice {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.incident_id.is_nil() && self.event_id > 0,
            "invalid incident identity"
        );
        ensure!(
            !self.service.is_empty()
                && self.service.len() <= 128
                && self
                    .service
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)),
            "invalid service name"
        );
        ensure!(
            matches!(
                self.state.as_str(),
                "acknowledged"
                    | "investigated"
                    | "unclassified"
                    | "condition_failed"
                    | "waiting_for_operator"
                    | "approved"
                    | "repairing"
                    | "failed"
                    | "recovered"
                    | "followup"
            ),
            "unknown incident state"
        );
        ensure!(
            self.summary.len() <= 2048 && !self.summary.chars().any(char::is_control),
            "invalid incident summary"
        );
        Ok(())
    }

    pub(crate) async fn receive(
        &self,
        store: &Store,
        instance: Uuid,
        chat: Uuid,
        sender: &str,
    ) -> Result<()> {
        self.validate()?;
        let id = Uuid::new_v5(
            &chat,
            &serde_json::to_vec(&(sender, self.incident_id, self.event_id))?,
        );
        let body = format!(
            "Incident {} · {} · {}\n{}\nFrom {}",
            self.incident_id,
            self.service,
            self.state.replace('_', " "),
            self.summary,
            sender
        );
        codifix_inbox::store::Store {
            pool: store.pool.clone(),
            home_instance_id: instance,
        }
        .receive_notice(chat, id, &body, None)
        .await
    }

    pub async fn send(&self, store: &Store, cfg: &Config, peer_id: &str) -> Result<()> {
        self.validate()?;
        let peer = cfg
            .peers
            .iter()
            .find(|p| p.id == peer_id)
            .context("unknown incident recipient")?;
        let body = serde_json::to_value(self)?;
        let receipt = crate::discovery::bounded(
            Transport::new(store, cfg, peer)?
                .call(Call::IncidentNotice(&body))
                .await?,
        )
        .await?;
        ensure!(
            receipt["received"] == true,
            "incident Inbox did not acknowledge receipt"
        );
        Ok(())
    }
}
