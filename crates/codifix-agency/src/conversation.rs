//! Versioned, bounded conversation snapshots and tool-free specialist proposals.
//! A home instance owns history and admission. A remote reply asserts no human
//! authentication and cannot itself grant permission to perform an action.
use crate::{PolicyError, ResourceScope, valid_identifier};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use uuid::Uuid;

pub const VERSION: u16 = 1;
pub const MAX_RESPONSE_TURNS: u32 = 8;
pub const ACCEPTANCE: &str = "conversation_reply_valid";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextRole {
    User,
    Specialist,
    Context,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextMessage {
    pub message_id: Uuid,
    pub role: ContextRole,
    pub speaker: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvailableAction {
    /// Operator-selected template key, never a model-selected mandate or URL.
    pub key: String,
    pub capability: String,
    pub resource: ResourceScope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvailablePeer {
    pub specialist_id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationTurn {
    pub version: u16,
    pub home_instance_id: Uuid,
    pub conversation_id: Uuid,
    pub root_message_id: Uuid,
    pub turn_id: Uuid,
    pub recipient_instance_id: Uuid,
    pub recipient_specialist_id: String,
    pub disclosure_digest: String,
    pub context: Vec<ContextMessage>,
    #[serde(default)]
    pub available_actions: Vec<AvailableAction>,
    #[serde(default)]
    pub available_peers: Vec<AvailablePeer>,
}

impl ConversationTurn {
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.version != VERSION
            || [
                self.home_instance_id,
                self.conversation_id,
                self.root_message_id,
                self.turn_id,
                self.recipient_instance_id,
            ]
            .iter()
            .any(Uuid::is_nil)
            || !valid_identifier(&self.recipient_specialist_id)
            || self.disclosure_digest.len() != 64
            || !self
                .disclosure_digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || self.context.is_empty()
            || self.context.len() > 128
            || self.available_actions.len() > 32
            || self.available_peers.len() > 16
        {
            return Err(PolicyError::Invalid("conversation snapshot"));
        }
        let mut ids = BTreeSet::new();
        let mut bytes = 0usize;
        for message in &self.context {
            if message.message_id.is_nil()
                || !ids.insert(message.message_id)
                || message.speaker.is_empty()
                || message.speaker.len() > 256
                || message.body.is_empty()
                || message.body.len() > 32768
            {
                return Err(PolicyError::Invalid("conversation context"));
            }
            bytes = bytes.saturating_add(message.body.len());
        }
        if bytes > 131072 {
            return Err(PolicyError::Invalid("conversation context size"));
        }
        let mut keys = BTreeSet::new();
        for action in &self.available_actions {
            if !valid_identifier(&action.key)
                || !valid_identifier(&action.capability)
                || !keys.insert(&action.key)
                || !valid_identifier(&action.resource.kind)
                || action.resource.id.is_empty()
                || action.resource.id.len() > 1024
            {
                return Err(PolicyError::Invalid("conversation action template"));
            }
        }
        keys.clear();
        for peer in &self.available_peers {
            if !valid_identifier(&peer.specialist_id)
                || !keys.insert(&peer.specialist_id)
                || peer.display_name.is_empty()
                || peer.display_name.len() > 256
            {
                return Err(PolicyError::Invalid("conversation peer"));
            }
        }
        Ok(())
    }

    /// The authenticated transport peer asserts the home identity. It does not
    /// authenticate a remote human principal or authorize any proposed action.
    pub fn validate_recipient(
        &self,
        sender: Uuid,
        recipient: Uuid,
        specialist: &str,
    ) -> Result<(), PolicyError> {
        self.validate()?;
        if self.home_instance_id != sender
            || self.recipient_instance_id != recipient
            || self.recipient_specialist_id != specialist
        {
            return Err(PolicyError::Scope("conversation identity"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionProposal {
    pub key: String,
    /// Input data only. The home resolves all authority and transport fields
    /// from the selected immutable template, never from this object.
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerRequest {
    pub specialist_id: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationReply {
    pub version: u16,
    pub reply: String,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub actions: Vec<ActionProposal>,
    #[serde(default)]
    pub peer_requests: Vec<PeerRequest>,
}

impl ConversationReply {
    pub fn validate_for(&self, turn: &ConversationTurn) -> Result<(), PolicyError> {
        turn.validate()?;
        if self.version != VERSION
            || self.reply.trim().is_empty()
            || self.reply.len() > 32768
            || self
                .question
                .as_ref()
                .is_some_and(|q| q.trim().is_empty() || q.len() > 8192)
            || self.actions.len() > 4
            || self.peer_requests.len() > 4
            || (self.question.is_some()
                && (!self.actions.is_empty() || !self.peer_requests.is_empty()))
        {
            return Err(PolicyError::Invalid("conversation reply"));
        }
        let mut keys = BTreeSet::new();
        for action in &self.actions {
            if !keys.insert(&action.key)
                || !turn.available_actions.iter().any(|a| a.key == action.key)
                || !action.payload.is_object()
                || serde_json::to_vec(&action.payload)
                    .map_err(|_| PolicyError::Invalid("action data"))?
                    .len()
                    > 65536
            {
                return Err(PolicyError::Scope("conversation action"));
            }
        }
        keys.clear();
        for peer in &self.peer_requests {
            if !keys.insert(&peer.specialist_id)
                || !turn
                    .available_peers
                    .iter()
                    .any(|p| p.specialist_id == peer.specialist_id)
                || peer.message.trim().is_empty()
                || peer.message.len() > 8192
            {
                return Err(PolicyError::Scope("conversation peer request"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn turn() -> ConversationTurn {
        ConversationTurn {
            version: VERSION,
            home_instance_id: Uuid::new_v4(),
            conversation_id: Uuid::new_v4(),
            root_message_id: Uuid::new_v4(),
            turn_id: Uuid::new_v4(),
            recipient_instance_id: Uuid::new_v4(),
            recipient_specialist_id: "planner".into(),
            disclosure_digest: "a".repeat(64),
            context: vec![ContextMessage {
                message_id: Uuid::new_v4(),
                role: ContextRole::User,
                speaker: "owner".into(),
                body: "Prepare the roadmap".into(),
            }],
            available_actions: vec![AvailableAction {
                key: "prepare".into(),
                capability: "plan_prepare".into(),
                resource: ResourceScope {
                    kind: "plan".into(),
                    id: "approved-plan".into(),
                },
            }],
            available_peers: vec![AvailablePeer {
                specialist_id: "reviewer".into(),
                display_name: "Reviewer".into(),
            }],
        }
    }
    #[test]
    fn transport_identity_and_context_bounds_are_checked() {
        let mut t = turn();
        assert!(
            t.validate_recipient(t.home_instance_id, t.recipient_instance_id, "planner")
                .is_ok()
        );
        assert!(
            t.validate_recipient(Uuid::new_v4(), t.recipient_instance_id, "planner")
                .is_err()
        );
        assert!(
            t.validate_recipient(t.home_instance_id, t.recipient_instance_id, "other")
                .is_err()
        );
        t.context.push(t.context[0].clone());
        assert!(t.validate().is_err());
    }
    #[test]
    fn proposals_cannot_invent_tools_peers_or_bypass_waiting_for_human() {
        let t = turn();
        let mut r = ConversationReply {
            version: VERSION,
            reply: "Roadmap ready".into(),
            question: None,
            actions: vec![ActionProposal {
                key: "prepare".into(),
                payload: serde_json::json!({"plan":{}}),
            }],
            peer_requests: vec![],
        };
        assert!(r.validate_for(&t).is_ok());
        r.actions[0].key = "deploy".into();
        assert!(r.validate_for(&t).is_err());
        r.actions.clear();
        r.peer_requests.push(PeerRequest {
            specialist_id: "stranger".into(),
            message: "hello".into(),
        });
        assert!(r.validate_for(&t).is_err());
        r.peer_requests[0].specialist_id = "reviewer".into();
        assert!(r.validate_for(&t).is_ok());
        r.question = Some("Which milestone?".into());
        assert!(r.validate_for(&t).is_err());
    }
}
