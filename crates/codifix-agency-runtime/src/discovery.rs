use crate::config::{Config, validate_endpoint};
use anyhow::{Result, ensure};
use codifix_agency::discovery::PublicCapabilityProfile;
use serde_json::{Value, json};

/// This deliberately uses a separately configured public account. Private
/// expertise repositories and records never enter this publication path.
pub async fn publish(cfg: &Config, pds: &str, public_did: &str, token: &str) -> Result<Value> {
    validate_endpoint(pds, false)?;
    ensure!(
        !cfg.private_repository_dids.iter().any(|d| d == public_did),
        "public and private repositories must differ"
    );
    ensure!(public_did.starts_with("did:"), "DID required");
    let record: PublicCapabilityProfile = serde_json::from_value(
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
    let endpoint = format!(
        "{}/xrpc/com.atproto.repo.putRecord",
        pds.trim_end_matches('/')
    );
    let receipt=bounded(client.post(endpoint).bearer_auth(token).json(&json!({"repo":public_did,"collection":record.record_type,"rkey":key,"record":record,"validate":false})).send().await?).await?;
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
        observed["value"] == serde_json::to_value(&record)? && observed["cid"] == receipt["cid"],
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
    let profile: PublicCapabilityProfile = serde_json::from_value(value["value"].clone())?;
    profile.validate()?;
    // Returned identity is discovery data, never a new peer/grant or an automatic connection.
    Ok(profile)
}
pub(crate) async fn bounded(mut response: reqwest::Response) -> Result<Value> {
    ensure!(
        response.status().is_success(),
        "remote endpoint rejected request"
    );
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
