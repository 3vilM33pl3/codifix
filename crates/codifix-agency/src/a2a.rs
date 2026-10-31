//! Text/data subset of A2A 1.0 JSON, not an A2A server. Field names, wrapper
//! unions and enum values follow https://a2a-protocol.org/v1.0.0/specification/.
//! Runtime still owns auth, method handling, version negotiation and tenancy.
use crate::{
    PolicyError, TaskContract, TaskStatus as InternalStatus,
    discovery::{PublicCapabilityProfile, validate_public_url},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const PROTOCOL_VERSION: &str = "1.0";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInterface {
    pub url: String,
    pub protocol_binding: String,
    pub protocol_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub streaming: bool,
    pub push_notifications: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub version: String,
    pub supported_interfaces: Vec<AgentInterface>,
    pub capabilities: AgentCapabilities,
    pub default_input_modes: Vec<String>,
    pub default_output_modes: Vec<String>,
    pub skills: Vec<AgentSkill>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub security_schemes: BTreeMap<String, SecurityScheme>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security_requirements: Vec<SecurityRequirement>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecurityScheme {
    HttpAuthSecurityScheme(HttpAuthSecurityScheme),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpAuthSecurityScheme {
    pub scheme: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringList {
    pub list: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityRequirement {
    pub schemes: BTreeMap<String, StringList>,
}

impl AgentCard {
    pub fn require_bearer(&mut self) {
        self.security_schemes.insert(
            "bearer".into(),
            SecurityScheme::HttpAuthSecurityScheme(HttpAuthSecurityScheme {
                scheme: "Bearer".into(),
            }),
        );
        self.security_requirements = vec![SecurityRequirement {
            schemes: BTreeMap::from([("bearer".into(), StringList::default())]),
        }];
    }

    /// Discovery-only card. The runtime must add its truthful security declarations
    /// before advertising an authenticated endpoint; this type carries no credentials.
    pub fn from_profile(
        profile: &PublicCapabilityProfile,
        endpoint: String,
        version: String,
        binding: &str,
    ) -> Result<Self, PolicyError> {
        profile.validate()?;
        validate_public_url(&endpoint)?;
        if !matches!(binding, "JSONRPC" | "HTTP+JSON") || version.trim().is_empty() {
            return Err(PolicyError::Invalid("A2A interface"));
        }
        Ok(Self {
            name: profile.display_name.clone(),
            description: profile.summary.clone(),
            version,
            supported_interfaces: vec![AgentInterface {
                url: endpoint,
                protocol_binding: binding.into(),
                protocol_version: PROTOCOL_VERSION.into(),
                tenant: None,
            }],
            security_schemes: BTreeMap::new(),
            security_requirements: Vec::new(),
            capabilities: AgentCapabilities::default(),
            default_input_modes: vec!["application/json".into()],
            default_output_modes: vec!["application/json".into()],
            skills: profile
                .capabilities
                .iter()
                .map(|id| AgentSkill {
                    id: id.clone(),
                    name: id.clone(),
                    description: profile.summary.clone(),
                    tags: vec!["codifix".into()],
                })
                .collect(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    #[serde(rename = "ROLE_USER")]
    User,
    #[serde(rename = "ROLE_AGENT")]
    Agent,
}
/// Supported content union. Optional wire mediaType/metadata are accepted but
/// intentionally not retained by this minimal content-only domain type.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Text(String),
    Data(Value),
}
impl Serialize for Part {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{Error, SerializeMap};
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            Self::Text(text) => map.serialize_entry("text", text)?,
            Self::Data(data) => {
                if !data.is_object() {
                    return Err(S::Error::custom("data part must be an object"));
                }
                map.serialize_entry("data", data)?;
            }
        }
        map.end()
    }
}
impl<'de> Deserialize<'de> for Part {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PartVisitor;
        impl<'de> serde::de::Visitor<'de> for PartVisitor {
            type Value = Part;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(
                    "an object containing exactly one supported text or object data part",
                )
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Part, A::Error> {
                use serde::de::Error;
                let mut content = None;
                let mut media_type_seen = false;
                let mut metadata_seen = false;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "text" | "data" => {
                            if content.is_some() {
                                return Err(A::Error::custom(
                                    "ambiguous or duplicate part content",
                                ));
                            }
                            content = Some(if key == "text" {
                                Part::Text(map.next_value::<String>()?)
                            } else {
                                let value = map.next_value::<Value>()?;
                                if !value.is_object() {
                                    return Err(A::Error::custom("data part must be an object"));
                                }
                                Part::Data(value)
                            });
                        }
                        "raw" | "url" => {
                            return Err(A::Error::custom("unsupported or ambiguous part content"));
                        }
                        "mediaType" => {
                            if media_type_seen {
                                return Err(A::Error::duplicate_field("mediaType"));
                            }
                            media_type_seen = true;
                            let _: String = map.next_value()?;
                        }
                        "metadata" => {
                            if metadata_seen {
                                return Err(A::Error::duplicate_field("metadata"));
                            }
                            metadata_seen = true;
                            let _: serde_json::Map<String, Value> = map.next_value()?;
                        }
                        _ => {
                            let _: serde::de::IgnoredAny = map.next_value()?;
                        }
                    }
                }
                content.ok_or_else(|| A::Error::custom("missing part content"))
            }
        }
        deserializer.deserialize_map(PartVisitor)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub message_id: String,
    pub role: Role,
    pub parts: Vec<Part>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}
impl Message {
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.message_id.trim().is_empty()
            || self.parts.is_empty()
            || self
                .parts
                .iter()
                .any(|part| matches!(part, Part::Data(value) if !value.is_object()))
            || self
                .context_id
                .as_ref()
                .is_some_and(|s| s.trim().is_empty())
            || self.task_id.as_ref().is_some_and(|s| s.trim().is_empty())
            || (self.role == Role::Agent && self.context_id.is_none())
        {
            return Err(PolicyError::Invalid("A2A message"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    #[serde(rename = "TASK_STATE_SUBMITTED")]
    Submitted,
    #[serde(rename = "TASK_STATE_WORKING")]
    Working,
    #[serde(rename = "TASK_STATE_COMPLETED")]
    Completed,
    #[serde(rename = "TASK_STATE_FAILED")]
    Failed,
    #[serde(rename = "TASK_STATE_CANCELED")]
    Canceled,
    #[serde(rename = "TASK_STATE_INPUT_REQUIRED")]
    InputRequired,
    #[serde(rename = "TASK_STATE_REJECTED")]
    Rejected,
    #[serde(rename = "TASK_STATE_AUTH_REQUIRED")]
    AuthRequired,
    #[serde(rename = "TASK_STATE_UNSPECIFIED")]
    Unspecified,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    pub timestamp: DateTime<Utc>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub artifact_id: String,
    pub parts: Vec<Part>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub context_id: String,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
}
impl Task {
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.id.trim().is_empty() || self.context_id.trim().is_empty() {
            return Err(PolicyError::Invalid("A2A task"));
        }
        let mut ids = BTreeSet::new();
        if self.artifacts.iter().any(|a| {
            a.artifact_id.trim().is_empty() || a.parts.is_empty() || !ids.insert(&a.artifact_id)
        }) {
            return Err(PolicyError::Invalid("A2A artifact"));
        }
        if let Some(message) = &self.status.message {
            message.validate()?;
            if message.context_id.as_ref() != Some(&self.context_id)
                || message.task_id.as_ref() != Some(&self.id)
            {
                return Err(PolicyError::Invalid("A2A status message binding"));
            }
        }
        Ok(())
    }

    /// PRIVATE task projection. Authorize the recipient before using; outcome
    /// references are intentionally not suitable for public discovery.
    pub fn from_contract(contract: &TaskContract, context_id: String) -> Result<Self, PolicyError> {
        contract.validate()?;
        if context_id.trim().is_empty() {
            return Err(PolicyError::Invalid("A2A context"));
        }
        let state = match contract.status {
            InternalStatus::Submitted => TaskState::Submitted,
            InternalStatus::Working => TaskState::Working,
            InternalStatus::InputRequired | InternalStatus::PlanReviewRequired => {
                TaskState::InputRequired
            }
            InternalStatus::Completed => TaskState::Completed,
            InternalStatus::Failed => TaskState::Failed,
            InternalStatus::Canceled => TaskState::Canceled,
            InternalStatus::Rejected => TaskState::Rejected,
        };
        let artifacts = contract
            .outcome
            .as_ref()
            .map(|o| {
                serde_json::to_value(o).map(|data| {
                    vec![Artifact {
                        artifact_id: format!("{}:outcome", contract.id),
                        parts: vec![Part::Data(data)],
                    }]
                })
            })
            .transpose()
            .map_err(|_| PolicyError::Invalid("A2A outcome"))?
            .unwrap_or_default();
        Ok(Self {
            id: contract.id.to_string(),
            context_id,
            status: TaskStatus {
                state,
                message: None,
                timestamp: contract.updated_at,
            },
            artifacts,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    pub message: Message,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<SendMessageConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}
/// A2A 1.0 defaults to waiting for a terminal or interrupted task. The runtime
/// must implement that behavior and explicitly reject unsupported push configs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageConfiguration {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_output_modes: Vec<String>,
    #[serde(default)]
    pub return_immediately: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_push_notification_config: Option<Value>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SendMessageResponse {
    Task(Task),
    Message(Message),
}

impl SendMessageRequest {
    pub fn validate(
        &self,
        interface: &AgentInterface,
        existing_task: Option<&Task>,
    ) -> Result<(), PolicyError> {
        self.message.validate()?;
        if self.configuration.as_ref().is_some_and(|config| {
            config
                .history_length
                .is_some_and(|length| length > i32::MAX as u32)
                || config
                    .accepted_output_modes
                    .iter()
                    .any(|mode| mode.trim().is_empty())
        }) {
            return Err(PolicyError::Invalid("A2A send configuration"));
        }
        if self.message.role != Role::User
            || self.tenant != interface.tenant
            || interface.protocol_version != PROTOCOL_VERSION
        {
            return Err(PolicyError::Invalid("A2A request interface"));
        }
        if let Some(task) = existing_task {
            task.validate()?;
            if self.message.task_id.as_ref() != Some(&task.id)
                || self
                    .message
                    .context_id
                    .as_ref()
                    .is_some_and(|id| id != &task.context_id)
            {
                return Err(PolicyError::Invalid("A2A task context binding"));
            }
        } else if self.message.task_id.is_some() {
            return Err(PolicyError::Invalid("unknown A2A task"));
        }
        Ok(())
    }
}
