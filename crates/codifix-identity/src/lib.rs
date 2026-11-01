// SPDX-License-Identifier: AGPL-3.0-or-later
//! The human service's narrow client for the isolated AT identity broker.
//! No OAuth credentials or caller-supplied identity assertions cross this API.
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use codifix_agency::discovery::PublicCapabilityProfile;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkBinding {
    pub attempt_id: Uuid,
    pub principal_id: Uuid,
    pub home_instance_id: Uuid,
    /// SHA-256 digest of the current Codifix session binding; never a bearer token.
    pub session_binding: String,
}
impl LinkBinding {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.attempt_id.is_nil()
                && !self.principal_id.is_nil()
                && !self.home_instance_id.is_nil(),
            "non-nil link binding required"
        );
        ensure!(
            self.session_binding.len() == 64
                && self
                    .session_binding
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid session binding"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkStart {
    pub authorization_url: String,
    pub expires_at: DateTime<Utc>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum LinkPoll {
    Pending,
    Complete {
        did: String,
        handle: Option<String>,
        pds: String,
        verified_at: DateTime<Utc>,
        cleanup: String,
    },
    Failed {
        code: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveredProfile {
    pub did: String,
    pub handle: Option<String>,
    pub pds: String,
    pub uri: String,
    pub cid: String,
    pub fetched_at: DateTime<Utc>,
    pub profile: PublicCapabilityProfile,
}
impl DiscoveredProfile {
    pub fn validate(&self) -> Result<()> {
        self.profile.validate()?;
        ensure!(
            self.did.starts_with("did:plc:") || self.did.starts_with("did:web:"),
            "unsupported DID"
        );
        ensure!(
            self.uri.starts_with(&format!(
                "at://{}/{}/",
                self.did,
                codifix_agency::discovery::CAPABILITY_COLLECTION
            )),
            "record/DID mismatch"
        );
        ensure!((10..=256).contains(&self.cid.len()), "invalid record CID");
        ensure!(
            reqwest::Url::parse(&self.pds)?.scheme() == "https",
            "HTTPS PDS required"
        );
        Ok(())
    }
}
#[derive(Clone)]
pub struct BrokerClient {
    client: reqwest::Client,
    socket: PathBuf,
}
impl BrokerClient {
    pub fn new(socket: impl AsRef<Path>) -> Result<Self> {
        let socket = socket.as_ref().to_path_buf();
        ensure!(
            socket.is_absolute(),
            "absolute identity broker socket required"
        );
        #[cfg(unix)]
        let client = reqwest::Client::builder()
            .unix_socket(socket.clone())
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(60))
            .build()?;
        #[cfg(not(unix))]
        let client = {
            anyhow::bail!("identity broker requires Unix sockets");
            #[allow(unreachable_code)]
            reqwest::Client::new()
        };
        Ok(Self { client, socket })
    }
    pub async fn begin(&self, binding: &LinkBinding, login_hint: &str) -> Result<LinkStart> {
        binding.validate()?;
        let result: LinkStart = self
            .post(
                "/internal/link/begin",
                &serde_json::json!({"binding":binding,"login_hint":login_hint}),
            )
            .await?;
        let url = reqwest::Url::parse(&result.authorization_url)?;
        ensure!(
            url.scheme() == "https" && url.username().is_empty() && url.password().is_none(),
            "invalid authorization URL"
        );
        ensure!(
            result.expires_at > Utc::now(),
            "expired authorization attempt"
        );
        Ok(result)
    }
    /// Returns the same completion until `acknowledge` follows a committed local link.
    pub async fn poll(&self, binding: &LinkBinding) -> Result<LinkPoll> {
        binding.validate()?;
        self.post(
            "/internal/link/poll",
            &serde_json::json!({"binding":binding}),
        )
        .await
    }
    pub async fn acknowledge(&self, binding: &LinkBinding) -> Result<()> {
        binding.validate()?;
        let _: serde_json::Value = self
            .post(
                "/internal/link/ack",
                &serde_json::json!({"binding":binding}),
            )
            .await?;
        Ok(())
    }
    pub async fn discover(&self, identifier: &str) -> Result<Vec<DiscoveredProfile>> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Profiles {
            profiles: Vec<DiscoveredProfile>,
        }
        let result: Profiles = self
            .post(
                "/internal/discover",
                &serde_json::json!({"identifier":identifier}),
            )
            .await?;
        ensure!(result.profiles.len() <= 100, "too many profiles");
        for profile in &result.profiles {
            profile.validate()?;
        }
        Ok(result.profiles)
    }
    pub async fn metadata(&self) -> Result<serde_json::Value> {
        self.socket_permissions()?;
        decode(
            self.client
                .get("http://identity.local/oauth/client-metadata.json")
                .send()
                .await?,
        )
        .await
    }
    /// Forward only the raw query from the fixed public callback route. Do not log it.
    pub async fn callback(&self, query: &str) -> Result<()> {
        ensure!(
            query.len() <= 8192 && !query.bytes().any(|b| b.is_ascii_control()),
            "invalid OAuth callback"
        );
        self.socket_permissions()?;
        let mut url = reqwest::Url::parse("http://identity.local/oauth/callback")?;
        url.set_query(Some(query));
        let value: serde_json::Value = decode(self.client.get(url).send().await?).await?;
        ensure!(
            value.get("ok") == Some(&serde_json::Value::Bool(true)),
            "callback rejected"
        );
        Ok(())
    }
    async fn post<T: DeserializeOwned>(&self, path: &str, body: &serde_json::Value) -> Result<T> {
        self.socket_permissions()?;
        decode(
            self.client
                .post(format!("http://identity.local{path}"))
                .json(body)
                .send()
                .await?,
        )
        .await
    }
    fn socket_permissions(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{FileTypeExt, MetadataExt};
            let socket = std::fs::symlink_metadata(&self.socket)?;
            ensure!(
                socket.file_type().is_socket() && socket.mode() & 0o007 == 0,
                "identity socket must deny access to other users"
            );
            let parent = std::fs::symlink_metadata(
                self.socket
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("socket parent missing"))?,
            )?;
            ensure!(
                parent.is_dir() && !parent.file_type().is_symlink() && parent.mode() & 0o022 == 0,
                "identity socket directory must not be group/world writable"
            );
        }
        Ok(())
    }
}
async fn decode<T: DeserializeOwned>(mut response: reqwest::Response) -> Result<T> {
    ensure!(
        response.status().is_success(),
        "identity broker rejected request"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= 262144,
            "identity response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_explicit_session_and_owner_binding() {
        let mut binding = LinkBinding {
            attempt_id: Uuid::new_v4(),
            principal_id: Uuid::new_v4(),
            home_instance_id: Uuid::new_v4(),
            session_binding: "a".repeat(64),
        };
        assert!(binding.validate().is_ok());
        binding.session_binding = "raw-session-token".into();
        assert!(binding.validate().is_err());
        binding.session_binding = "a".repeat(64);
        binding.principal_id = Uuid::nil();
        assert!(binding.validate().is_err());
    }
    #[test]
    fn oauth_tokens_cannot_appear_in_completion_contract() {
        let value = serde_json::json!({"status":"complete","did":"did:plc:abcdefghijklmnopqrstuvwx","handle":null,"pds":"https://pds.codifix.net","verified_at":Utc::now(),"cleanup":"revoked","access_token":"secret"});
        assert!(serde_json::from_value::<LinkPoll>(value).is_err());
    }
    #[test]
    fn relative_socket_is_rejected() {
        assert!(BrokerClient::new("identity.sock").is_err());
    }
}
