//! Private, service-authenticated AT Protocol calls between enrolled companions.
use crate::config::{Config, Peer, validate_endpoint};
use anyhow::{Result, ensure};
use atrium_crypto::{Algorithm, did::parse_did_key, verify::verify_signature};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocialConfig {
    pub did: String,
    pub pds: String,
    pub access_token_env: String,
    pub peers: BTreeMap<String, SocialPeer>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocialPeer {
    #[serde(default)]
    pub incident_conversation: Option<uuid::Uuid>,
    #[serde(default)]
    pub notice_inbox: Option<NoticeInbox>,
    pub did: String,
    pub signing_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoticeInbox {
    pub conversation_id: uuid::Uuid,
    pub unseal_url: Option<String>,
}

pub fn service_did(account_did: &str) -> String {
    format!("{account_did}#codifix_agency")
}

impl SocialConfig {
    // The local PDS is trusted via TLS. Catch an accidentally swapped account before sending private content.
    pub(crate) fn check_outbound_identity(
        &self,
        token: &str,
        recipient: &str,
        method: &str,
    ) -> Result<()> {
        ensure!(token.len() <= 4096, "oversized PDS token");
        let parts: Vec<_> = token.split('.').collect();
        ensure!(parts.len() == 3, "invalid PDS token");
        let claims: Claims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1])?)?;
        ensure!(
            claims.iss == self.did && claims.aud == recipient && claims.lxm == method,
            "PDS token identity mismatch"
        );
        Ok(())
    }

    pub(crate) fn validate(&self, cfg: &Config) -> Result<()> {
        validate_endpoint(&self.pds, true)?;
        ensure!(
            valid_did(&self.did) && !self.access_token_env.is_empty(),
            "invalid local AT identity"
        );
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for (id, binding) in &self.peers {
            ensure!(
                cfg.peers
                    .iter()
                    .any(|p| &p.id == id && !p.operator && p.instance_id != cfg.instance_id),
                "social peer must be an enrolled remote specialist"
            );
            ensure!(
                valid_did(&binding.did) && binding.did != self.did && ids.insert(&binding.did),
                "duplicate or invalid peer DID"
            );
            if let Some(id) = binding.incident_conversation {
                ensure!(
                    cfg.inbox_enabled && !id.is_nil(),
                    "incident Inbox must be enabled"
                );
            }
            if let Some(inbox) = &binding.notice_inbox {
                ensure!(
                    cfg.inbox_enabled && !inbox.conversation_id.is_nil(),
                    "notice Inbox must be enabled"
                );
                if let Some(value) = &inbox.unseal_url {
                    validate_endpoint(value, false)?;
                    let url = url::Url::parse(value)?;
                    ensure!(
                        url.path() == "/unseal" && url.query().is_none(),
                        "expected protected unseal page"
                    );
                }
            }
            parse_did_key(&binding.signing_key)?;
            ensure!(keys.insert(&binding.signing_key), "shared peer signing key");
        }
        Ok(())
    }

    pub(crate) fn verify<'a>(
        &self,
        peers: &'a [Peer],
        token: &str,
        method: &str,
        now: i64,
    ) -> Result<(&'a Peer, Claims)> {
        ensure!(token.len() <= 4096, "oversized token");
        let (signed, signature) = token
            .rsplit_once('.')
            .ok_or_else(|| anyhow::anyhow!("invalid token"))?;
        let (header, body) = signed
            .split_once('.')
            .ok_or_else(|| anyhow::anyhow!("invalid token"))?;
        let header: Header = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header)?)?;
        let claims: Claims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body)?)?;
        ensure!(
            claims.aud == service_did(&self.did)
                && claims.lxm == method
                && claims.exp > now
                && claims.exp <= now + 90
                && !claims.jti.is_empty()
                && claims.jti.len() <= 128,
            "invalid token scope"
        );
        let (id, binding) = self
            .peers
            .iter()
            .find(|(_, binding)| binding.did == claims.iss)
            .ok_or_else(|| anyhow::anyhow!("unenrolled issuer"))?;
        let peer = peers
            .iter()
            .find(|peer| &peer.id == id && !peer.operator)
            .ok_or_else(|| anyhow::anyhow!("unenrolled peer"))?;
        let (algorithm, _) = parse_did_key(&binding.signing_key)?;
        ensure!(
            matches!(
                (algorithm, header.alg.as_str()),
                (Algorithm::Secp256k1, "ES256K") | (Algorithm::P256, "ES256")
            ),
            "invalid signature algorithm"
        );
        verify_signature(
            &binding.signing_key,
            signed.as_bytes(),
            &URL_SAFE_NO_PAD.decode(signature)?,
        )?;
        Ok((peer, claims))
    }
}

fn valid_did(value: &str) -> bool {
    (value.starts_with("did:plc:") || value.starts_with("did:web:"))
        && value.len() > 8
        && value.len() <= 2048
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'%' | b'.' | b'-' | b'_'))
}

#[derive(Deserialize)]
struct Header {
    alg: String,
}
#[derive(Deserialize)]
pub(crate) struct Claims {
    iss: String,
    aud: String,
    lxm: String,
    pub jti: String,
    pub exp: i64,
}
