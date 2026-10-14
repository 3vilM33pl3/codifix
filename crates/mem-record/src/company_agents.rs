// SPDX-License-Identifier: AGPL-3.0-or-later
//! Independent child identities. These receipts are metadata, not MCP or export authority.
use crate::{CompanyDeviceSelection, OnboardingDestination};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompanyAgentReplicationMode {
    #[default]
    SharedPublicationsV1,
    CuratedProjectV1,
}
impl CompanyAgentReplicationMode {
    pub fn is_shared(&self) -> bool {
        *self == Self::SharedPublicationsV1
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SharedPublicationsV1 => "shared_publications_v1",
            Self::CuratedProjectV1 => "curated_project_v1",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompanyAgentCaller {
    Codex,
    Claude,
}
impl CompanyAgentCaller {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
    pub fn provider_id(self) -> &'static str {
        match self {
            Self::Codex => "openai",
            Self::Claude => "anthropic",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompanyAgentApproveRequest {
    #[serde(
        default,
        skip_serializing_if = "CompanyAgentReplicationMode::is_shared"
    )]
    pub replication_mode: CompanyAgentReplicationMode,
    pub request_id: Uuid,
    pub agent_id: Uuid,
    pub principal_id: Uuid,
    pub token_id: Uuid,
    pub caller: CompanyAgentCaller,
    pub provider_id: String,
    pub credential_sha256: String,
    pub expires_at: DateTime<Utc>,
    pub projects: Vec<CompanyDeviceSelection>,
    pub confirmed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyAgentScope {
    pub selection: CompanyDeviceSelection,
    pub destination: OnboardingDestination,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyAgentReceipt {
    #[serde(
        default,
        skip_serializing_if = "CompanyAgentReplicationMode::is_shared"
    )]
    pub replication_mode: CompanyAgentReplicationMode,
    pub schema_version: u32,
    pub request_id: Uuid,
    pub agent_id: Uuid,
    pub device_id: Uuid,
    pub human_id: Uuid,
    pub issuer: String,
    pub subject: String,
    pub company_id: Uuid,
    pub principal_id: Uuid,
    pub token_id: Uuid,
    pub caller: CompanyAgentCaller,
    pub provider_id: String,
    pub credential_sha256: String,
    pub expires_at: DateTime<Utc>,
    pub projects: Vec<CompanyAgentScope>,
    pub approved_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyAgentList {
    pub schema_version: u32,
    pub agents: Vec<CompanyAgentReceipt>,
}
