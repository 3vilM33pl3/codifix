use crate::config::{Config, validate_endpoint};
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use codifix_agency::discovery::{CapabilityExpiry, PublicCapabilityProfile};
use serde_json::{Value, json};

/// This deliberately uses a separately configured public account. Private
/// expertise repositories and records never enter this publication path.
pub async fn publish(cfg: &Config, pds: &str, public_did: &str, token: &str) -> Result<Value> {
    write_profile(cfg, pds, public_did, token, None).await
}

/// Update one reviewed announcement without overwriting a concurrent change.
pub async fn availability(
    cfg: &Config,
    pds: &str,
    did: &str,
    token: &str,
    capability: &str,
    on: bool,
    expires_at: Option<DateTime<Utc>>,
) -> Result<Value> {
    ensure!(
        expires_at.is_none_or(|expiry| on && expiry > Utc::now()),
        "expiry must be in the future for an enabled capability"
    );
    write_profile(cfg, pds, did, token, Some((capability, on, expires_at))).await
}

async fn write_profile(
    cfg: &Config,
    pds: &str,
    public_did: &str,
    token: &str,
    change: Option<(&str, bool, Option<DateTime<Utc>>)>,
) -> Result<Value> {
    validate_endpoint(pds, false)?;
    ensure!(
        !cfg.private_repository_dids.iter().any(|d| d == public_did),
        "public and private repositories must differ"
    );
    ensure!(public_did.starts_with("did:"), "DID required");
    let mut record: PublicCapabilityProfile = serde_json::from_value(
        cfg.public_profile
            .clone()
            .ok_or_else(|| anyhow::anyhow!("reviewed public profile required"))?,
    )?;
    record.validate()?;
    ensure!(
        record.instance_id == cfg.instance_id,
        "profile instance mismatch"
    );
    let manifest = cfg
        .specialists
        .iter()
        .find(|m| m.specialist_id == record.specialist_id)
        .ok_or_else(|| anyhow::anyhow!("unknown specialist"))?;
    ensure!(
        record.capabilities.is_subset(&manifest.capabilities),
        "profile capability mismatch"
    );
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let session = bounded(
        client
            .get(format!(
                "{}/xrpc/com.atproto.server.getSession",
                pds.trim_end_matches('/')
            ))
            .bearer_auth(token)
            .send()
            .await?,
    )
    .await?;
    ensure!(
        session["did"].as_str() == Some(public_did),
        "public account credential DID mismatch"
    );
    let key = format!("{}-{}", record.instance_id.simple(), record.specialist_id);
    let base = pds.trim_end_matches('/');
    let address = [
        ("repo", public_did),
        ("collection", record.record_type.as_str()),
        ("rkey", key.as_str()),
    ];
    let mut body =
        json!({"repo":public_did,"collection":record.record_type,"rkey":key,"validate":false});
    let mut method = "putRecord";
    if let Some((name, on, expires_at)) = change {
        ensure!(
            record.capabilities.contains(name),
            "capability lacks public disclosure approval"
        );
        let response = client
            .get(format!("{base}/xrpc/com.atproto.repo.getRecord"))
            .query(&address)
            .send()
            .await?;
        if response.status().is_success() {
            let current = bounded(response).await?;
            ensure!(
                current["uri"] == format!("at://{public_did}/{}/{key}", record.record_type),
                "unexpected profile URI"
            );
            let cid = current["cid"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing profile CID"))?;
            let mut prior: PublicCapabilityProfile =
                serde_json::from_value(current["value"].clone())?;
            prior.validate()?;
            ensure!(
                prior.instance_id == record.instance_id
                    && prior.specialist_id == record.specialist_id,
                "profile identity mismatch"
            );
            prior.retain_available(Utc::now());
            record
                .capabilities
                .retain(|cap| prior.capabilities.contains(cap));
            record.expirations = prior
                .expirations
                .into_iter()
                .filter(|e| record.capabilities.contains(&e.capability))
                .collect();
            body["swapRecord"] = json!(cid);
        } else {
            let error = bounded_json(response).await?;
            ensure!(
                error["error"] == "RecordNotFound",
                "could not read current announcement"
            );
            if !on {
                return Ok(json!({"available":false,"published":false}));
            }
            record.capabilities.clear();
            record.expirations.clear();
            method = "createRecord";
        }
        record.expirations.retain(|entry| entry.capability != name);
        if on {
            record.capabilities.insert(name.to_owned());
            if let Some(expires_at) = expires_at {
                record.expirations.push(CapabilityExpiry {
                    capability: name.to_owned(),
                    expires_at,
                });
            }
        } else {
            record.capabilities.remove(name);
        }
        record.published_at = Utc::now();
    }
    record.validate()?;
    body["record"] = serde_json::to_value(&record)?;
    let receipt = bounded(
        client
            .post(format!("{base}/xrpc/com.atproto.repo.{method}"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await?,
    )
    .await?;
    let observed = bounded(
        client
            .get(format!(
                "{}/xrpc/com.atproto.repo.getRecord",
                pds.trim_end_matches('/')
            ))
            .query(&[
                ("repo", public_did),
                ("collection", record.record_type.as_str()),
                ("rkey", key.as_str()),
            ])
            .send()
            .await?,
    )
    .await?;
    ensure!(
        observed["value"] == serde_json::to_value(&record)?
            && observed["uri"] == format!("at://{public_did}/{}/{key}", record.record_type)
            && receipt["cid"].as_str().is_some_and(|cid| !cid.is_empty())
            && observed["cid"] == receipt["cid"],
        "public profile readback mismatch"
    );
    Ok(json!({"uri":observed["uri"],"cid":observed["cid"],"public":true}))
}
pub async fn discover(pds: &str, did: &str, rkey: &str) -> Result<PublicCapabilityProfile> {
    validate_endpoint(pds, false)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let value = bounded(
        client
            .get(format!(
                "{}/xrpc/com.atproto.repo.getRecord",
                pds.trim_end_matches('/')
            ))
            .query(&[
                ("repo", did),
                (
                    "collection",
                    codifix_agency::discovery::CAPABILITY_COLLECTION,
                ),
                ("rkey", rkey),
            ])
            .send()
            .await?,
    )
    .await?;
    let expected = format!(
        "at://{did}/{}/{rkey}",
        codifix_agency::discovery::CAPABILITY_COLLECTION
    );
    ensure!(
        value["uri"].as_str() == Some(&expected),
        "profile URI mismatch"
    );
    let mut profile: PublicCapabilityProfile = serde_json::from_value(value["value"].clone())?;
    profile.validate()?;
    profile.retain_available(chrono::Utc::now());
    // Returned identity is discovery data, never a new peer/grant or an automatic connection.
    Ok(profile)
}
pub(crate) async fn bounded(response: reqwest::Response) -> Result<Value> {
    ensure!(
        response.status().is_success(),
        "remote endpoint rejected request"
    );
    bounded_json(response).await
}
async fn bounded_json(mut response: reqwest::Response) -> Result<Value> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= 1_048_576,
            "remote response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
}
