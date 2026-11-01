//! Private, human-owned conversations. Discovery and membership are not authority.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
pub mod store;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Specialist {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub specialist_id: String,
    pub display_name: String,
    pub summary: String,
    pub capabilities: Vec<String>,
    pub enrolled: bool,
    pub available: bool,
    pub grant_id: Option<Uuid>,
    pub mandate_id: Option<Uuid>,
    pub remote: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub title: String,
    pub lead_member_id: Option<Uuid>,
    pub paused: bool,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub unread_count: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Member {
    pub id: Uuid,
    pub specialist: Specialist,
    pub active: bool,
    pub revision: i64,
    pub joined_sequence: i64,
    pub disclosure_summary: String,
    pub selected_message_ids: Vec<Uuid>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sequence: i64,
    pub role: String,
    pub member_id: Option<Uuid>,
    pub body: String,
    pub format: String,
    pub artifact: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub delivery: String,
    pub root_id: Uuid,
    pub recipient_member_ids: Vec<Uuid>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessagePage {
    pub messages: Vec<Message>,
    pub has_more: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskCard {
    pub id: Uuid,
    pub member_id: Uuid,
    pub status: String,
    pub objective: String,
    pub question: Option<String>,
    pub result: Option<Value>,
    pub artifacts: Vec<Value>,
    pub cancel_requested: bool,
    pub root_id: Uuid,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Detail {
    pub conversation: Conversation,
    pub members: Vec<Member>,
    pub messages: Vec<Message>,
    pub tasks: Vec<TaskCard>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateConversation {
    pub client_request_id: Uuid,
    pub title: String,
    pub specialist_id: Uuid,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendMessage {
    pub client_message_id: Uuid,
    pub body: String,
    pub format: String,
    #[serde(default)]
    pub artifact: Option<Value>,
    #[serde(default)]
    pub recipient_member_ids: Vec<Uuid>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddMember {
    pub client_request_id: Uuid,
    pub specialist_id: Uuid,
    pub summary: String,
    pub selected_message_ids: Vec<Uuid>,
    pub confirmed: bool,
}
/// Operator-installed exact request templates; model output cannot supply authority fields.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionTemplate {
    pub key: String,
    pub request: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationGrant {
    pub id: Uuid,
    pub home_instance_id: Uuid,
    pub principal_id: Uuid,
    pub contact: Specialist,
    pub request: Value,
    pub routing: Value,
    #[serde(default)]
    pub actions: Vec<ActionTemplate>,
    pub expires_at: DateTime<Utc>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DispatchTicket {
    pub id: Uuid,
    pub request: Value,
    pub peer_id: Option<String>,
}
