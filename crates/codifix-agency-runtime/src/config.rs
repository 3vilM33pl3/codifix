use anyhow::{Result, ensure};
use codifix_agency::{SpecialistManifest, routing::ModelProfile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, net::SocketAddr, path::Path};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Metadata-only process heartbeat. Requires separately applied observation migration.
    #[serde(default)]
    pub observation_enabled: bool,
    /// Redeem same-database human inbox tickets. No HTTP peer receives this identity.
    #[serde(default)]
    pub inbox_enabled: bool,
    pub instance_id: Uuid,
    pub listen: SocketAddr,
    pub public_url: String,
    pub database_env: String,
    pub specialists: Vec<SpecialistManifest>,
    pub peers: Vec<Peer>,
    #[serde(default)]
    pub models: Vec<ModelEndpoint>,
    #[serde(default)]
    pub model_profiles: Vec<ModelProfile>,
    #[serde(default)]
    pub public_profile: Option<serde_json::Value>,
    #[serde(default)]
    pub private_repository_dids: Vec<String>,
    #[serde(default)]
    pub linear: Option<serde_json::Value>,
    #[serde(default)]
    pub openbao_controllers: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Peer {
    pub id: String,
    pub instance_id: Uuid,
    pub token_env: String,
    #[serde(default)]
    pub operator: bool,
    #[serde(default)]
    pub mandates: BTreeSet<Uuid>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub outbound_token_env: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEndpoint {
    pub profile_id: String,
    pub url: String,
    pub token_env: Option<String>,
    pub model: String,
    pub max_input_bytes: usize,
    pub max_output_tokens: u32,
    /// Operator-reviewed upper bound, not the empirical mean used by routing.
    pub reserve_micro_usd: u64,
    pub input_micro_usd_per_million: u64,
    pub output_micro_usd_per_million: u64,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        ensure!(bytes.len() <= 1_048_576, "configuration too large");
        let cfg: Self = serde_json::from_slice(&bytes)?;
        cfg.validate()?;
        Ok(cfg)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.instance_id.is_nil(), "instance identity required");
        ensure!(
            self.listen.ip().is_loopback(),
            "bind loopback behind a TLS reverse proxy"
        );
        validate_endpoint(&self.public_url, false)?;
        let mut ids = BTreeSet::new();
        for s in &self.specialists {
            s.validate()?;
            ensure!(
                s.instance_id == self.instance_id && ids.insert(&s.specialist_id),
                "invalid specialist binding"
            );
        }
        let mut peers = BTreeSet::new();
        for p in &self.peers {
            ensure!(
                !p.id.is_empty()
                    && !p.id.starts_with("inbox-gateway:")
                    && peers.insert(&p.id)
                    && !p.instance_id.is_nil(),
                "invalid peer identity"
            );
            if let Some(url) = &p.endpoint {
                validate_endpoint(url, false)?;
            }
        }
        let mut models = BTreeSet::new();
        for m in &self.models {
            validate_endpoint(&m.url, true)?;
            ensure!(
                models.insert(&m.profile_id)
                    && !m.model.is_empty()
                    && (1..=131072).contains(&m.max_input_bytes)
                    && (1..=16384).contains(&m.max_output_tokens)
                    && m.reserve_micro_usd > 0,
                "invalid model bounds"
            );
            let profile = self
                .model_profiles
                .iter()
                .find(|p| p.id == m.profile_id)
                .ok_or_else(|| anyhow::anyhow!("missing evaluated model profile"))?;
            ensure!(
                profile.model_revision == m.model,
                "model profile revision mismatch"
            );
            if matches!(
                profile.destination,
                codifix_agency::ModelDestination::Cloud { .. }
            ) {
                ensure!(
                    url::Url::parse(&m.url)?.scheme() == "https",
                    "cloud models require TLS"
                );
            }
            let bound = ((profile.context_tokens as u128 * m.input_micro_usd_per_million as u128)
                + (m.max_output_tokens as u128 * m.output_micro_usd_per_million as u128))
                .div_ceil(1_000_000);
            ensure!(
                m.reserve_micro_usd as u128 >= bound,
                "model reservation does not cover configured token bound"
            );
        }
        Ok(())
    }
}
pub fn validate_endpoint(value: &str, local_http: bool) -> Result<()> {
    let u = url::Url::parse(value)?;
    ensure!(
        u.username().is_empty() && u.password().is_none() && u.fragment().is_none(),
        "endpoint must not contain credentials or fragments"
    );
    let loopback = u
        .host_str()
        .is_some_and(|h| matches!(h, "localhost" | "127.0.0.1" | "[::1]" | "::1"));
    ensure!(
        u.scheme() == "https" || (local_http && u.scheme() == "http" && loopback),
        "endpoint requires TLS"
    );
    Ok(())
}

/// Only hashes are retained by the server's credential verifier. Tokens never
/// enter task payloads, model inputs, public cards or audit rows.
#[derive(Clone)]
pub struct Auth {
    credentials: Vec<(Peer, [u8; 32])>,
    cursor_key: [u8; 32],
}
impl Auth {
    pub fn from_config(cfg: &Config) -> Result<Self> {
        let pairs = cfg
            .peers
            .iter()
            .map(|peer| {
                let token = std::env::var(&peer.token_env)
                    .map_err(|_| anyhow::anyhow!("required peer credential unavailable"))?;
                Ok((peer.clone(), token))
            })
            .collect::<Result<Vec<_>>>()?;
        Self::from_credentials(pairs)
    }
    pub fn from_credentials(pairs: Vec<(Peer, String)>) -> Result<Self> {
        let mut credentials = Vec::new();
        let mut seen = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for (peer, token) in pairs {
            ensure!(
                token.len() >= 32
                    && !peer.id.starts_with("inbox-gateway:")
                    && ids.insert(peer.id.clone()),
                "invalid peer credential"
            );
            let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
            ensure!(seen.insert(digest), "each peer needs a distinct credential");
            credentials.push((peer, digest));
        }
        Ok(Self {
            credentials,
            cursor_key: Sha256::digest(Uuid::new_v4().as_bytes()).into(),
        })
    }
    pub fn encode_cursor(&self, value: &impl Serialize) -> Result<String> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        use hmac::{Hmac, Mac};
        let body = serde_json::to_vec(value)?;
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.cursor_key)?;
        mac.update(&body);
        Ok(format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(body),
            URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
        ))
    }
    pub fn decode_cursor<T: serde::de::DeserializeOwned>(&self, token: &str) -> Result<T> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        use hmac::{Hmac, Mac};
        ensure!(token.len() <= 2048, "invalid cursor");
        let (body, tag) = token
            .split_once('.')
            .ok_or_else(|| anyhow::anyhow!("invalid cursor"))?;
        let body = URL_SAFE_NO_PAD.decode(body)?;
        let tag = URL_SAFE_NO_PAD.decode(tag)?;
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.cursor_key)?;
        mac.update(&body);
        mac.verify_slice(&tag)?;
        Ok(serde_json::from_slice(&body)?)
    }
    pub fn authenticate(&self, bearer: &str) -> Option<Peer> {
        let digest: [u8; 32] = Sha256::digest(bearer.as_bytes()).into();
        self.credentials.iter().find_map(|(peer, stored)| {
            let difference = stored.iter().zip(digest).fold(0u8, |d, (a, b)| d | (a ^ b));
            (difference == 0).then(|| peer.clone())
        })
    }
    #[cfg(test)]
    pub fn testing(peer: Peer, token: &str) -> Self {
        Self {
            credentials: vec![(peer, Sha256::digest(token.as_bytes()).into())],
            cursor_key: Sha256::digest(Uuid::new_v4().as_bytes()).into(),
        }
    }
}
