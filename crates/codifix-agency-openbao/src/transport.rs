use crate::*;
use reqwest::Client;
use std::time::Duration;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub(crate) fn token(path: &std::path::Path) -> Result<Zeroizing<String>> {
    let bytes = protected::read(path)?;
    let s = std::str::from_utf8(&bytes)
        .map_err(|_| Error::ProtectedFile)?
        .trim();
    if s.len() < 32
        || s.len() > 256
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err(Error::ProtectedFile);
    }
    Ok(Zeroizing::new(s.to_string()))
}
pub(crate) fn authenticate(path: &std::path::Path, headers: &axum::http::HeaderMap) -> Result<()> {
    let expected = token(path)?;
    let received = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(Error::Authentication)?;
    let a = Sha256::digest(expected.as_bytes());
    let b = Sha256::digest(received.as_bytes());
    if bool::from(a.ct_eq(&b)) {
        Ok(())
    } else {
        Err(Error::Authentication)
    }
}

pub(crate) fn client(endpoint: &Endpoint) -> Result<Client> {
    endpoint.validate()?;
    let mut builder = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8))
        .connect_timeout(Duration::from_secs(3))
        .tls_built_in_root_certs(false);
    if let Some(path) = &endpoint.ca_pem_file {
        let pem = protected::read(path)?;
        let cert = reqwest::Certificate::from_pem(&pem).map_err(|_| Error::Configuration)?;
        builder = builder.add_root_certificate(cert);
    }
    builder.build().map_err(|_| Error::Transport)
}
pub(crate) fn url(endpoint: &Endpoint, path: &str) -> Result<reqwest::Url> {
    reqwest::Url::parse(&endpoint.url)
        .and_then(|u| u.join(path))
        .map_err(|_| Error::Configuration)
}
pub(crate) async fn decode<T: serde::de::DeserializeOwned>(
    mut response: reqwest::Response,
) -> Result<T> {
    if !response.status().is_success() {
        return Err(Error::Transport);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
        if bytes.len() + chunk.len() > 65536 {
            return Err(Error::Transport);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| Error::Transport)
}

pub async fn verified_status(target: &Target) -> Result<SealStatus> {
    target.validate(now())?;
    let http = client(&target.endpoint)?;
    let credential = token(&target.attestor_token_file)?;
    let identity: Identity = decode(
        client(&target.identity_endpoint)?
            .get(url(
                &target.identity_endpoint,
                "/codifix-agency/v1/identity",
            )?)
            .bearer_auth(&*credential)
            .send()
            .await
            .map_err(|_| Error::Transport)?,
    )
    .await?;
    if identity != target.expected {
        return Err(Error::TargetChanged);
    }
    let status: SealStatus = decode(
        http.get(url(&target.endpoint, "/v1/sys/seal-status")?)
            .send()
            .await
            .map_err(|_| Error::Transport)?,
    )
    .await?;
    status.check(target)?;
    Ok(status)
}

/// Holds a private state lock across the bounded ceremony. The controller never
/// receives a share; only each custodian talks to the native unseal endpoint.
pub async fn controller_tick(
    config: &ControllerConfig,
    mut observe: impl FnMut(Observation),
) -> Result<SealStatus> {
    let result = controller_inner(config, &mut observe).await;
    if let Err(e) = &result {
        let code = match e {
            Error::TargetChanged => "target_changed",
            Error::Incomplete => "incomplete_ceremony",
            Error::Ceremony => "invalid_ceremony",
            Error::Transport => "transport_unavailable",
            Error::ProtectedFile => "protected_state_unavailable",
            Error::Configuration => "invalid_enrollment",
            Error::Busy => "state_locked",
            _ => "operator_required",
        };
        observe(Observation::Escalation { code: code.into() });
    }
    result
}
async fn controller_inner(
    config: &ControllerConfig,
    observe: &mut impl FnMut(Observation),
) -> Result<SealStatus> {
    config.target.validate(now())?;
    validate_roster(&config.roster, &config.target)?;
    let peer_ids: BTreeSet<_> = config.peers.iter().map(|p| &p.custodian_id).collect();
    if peer_ids.len() != config.peers.len()
        || peer_ids.len() != config.roster.len()
        || config
            .roster
            .iter()
            .any(|m| !peer_ids.contains(&m.custodian_id))
    {
        return Err(Error::Configuration);
    }
    for p in &config.peers {
        p.endpoint.validate()?;
    }
    let _lock = protected::StateLock::acquire(&config.state_file)?;
    let mut state: ControllerState = protected::load(&config.state_file)?;
    let binding = config.target.binding()?;
    if state.binding != binding {
        return Err(Error::TargetChanged);
    }
    let status = verified_status(&config.target).await?;
    if !status.sealed {
        state.active = None;
        protected::save(&config.state_file, &state)?;
        observe(Observation::Unsealed);
        return Ok(status);
    }
    // Partial native ceremonies, an interrupted prior controller, and migration
    // never trigger a new automatic attempt. No reset endpoint is called.
    if state.active.is_some() || status.progress != 0 {
        return Err(Error::Incomplete);
    }
    state.sequence = state.sequence.checked_add(1).ok_or(Error::Ceremony)?;
    let issued = now();
    let ceremony = Ceremony {
        challenge: uuid::Uuid::new_v4().to_string(),
        sequence: state.sequence,
        issued_at: issued,
        expires_at: issued + 120,
        target_binding: binding,
        selected: config
            .roster
            .iter()
            .take(config.target.expected.threshold as usize)
            .map(|m| m.custodian_id.clone())
            .collect(),
    };
    ceremony.validate(&config.target, &config.roster, now())?;
    state.active = Some(ceremony.clone());
    protected::save(&config.state_file, &state)?;
    observe(Observation::CeremonyStarted {
        sequence: ceremony.sequence,
        challenge: ceremony.challenge.clone(),
    });
    for id in &ceremony.selected {
        if now() >= ceremony.expires_at {
            return Err(Error::Ceremony);
        }
        let p = config
            .peers
            .iter()
            .find(|p| &p.custodian_id == id)
            .ok_or(Error::Configuration)?;
        let secret = token(&p.controller_token_file)?;
        let response = client(&p.endpoint)?
            .post(url(&p.endpoint, "/v1/ceremony")?)
            .bearer_auth(&*secret)
            .json(&ceremony)
            .send()
            .await
            .map_err(|_| Error::Transport)?;
        let ack: crate::service::Acknowledgement = decode(response).await?;
        if ack.custodian_id != *id
            || ack.sequence != ceremony.sequence
            || ack.challenge != ceremony.challenge
        {
            return Err(Error::Ceremony);
        }
        observe(Observation::CustodianSubmitted {
            custodian_id: id.clone(),
            sequence: ceremony.sequence,
        });
        let status = verified_status(&config.target).await?;
        if !status.sealed {
            state.active = None;
            protected::save(&config.state_file, &state)?;
            observe(Observation::Unsealed);
            return Ok(status);
        }
    }
    Err(Error::Incomplete)
}

/// Authenticated server calls this after verifying its scoped controller token.
/// Replay consumption is durable *before* reading the share or touching OpenBao.
pub async fn custodian_submit(config: &CustodianConfig, ceremony: &Ceremony) -> Result<()> {
    config.validate(now())?;
    ceremony.validate(&config.target, &config.roster, now())?;
    if !ceremony.selected.contains(&config.custodian_id) {
        return Err(Error::Ceremony);
    }
    let _lock = protected::StateLock::acquire(&config.state_file)?;
    let mut state: CustodianState = protected::load(&config.state_file)?;
    if state.binding != config.target.binding()? {
        return Err(Error::TargetChanged);
    }
    if ceremony.sequence <= state.last_sequence
        || state.last_challenge.as_ref() == Some(&ceremony.challenge)
    {
        return Err(Error::Ceremony);
    }
    let status = verified_status(&config.target).await?;
    if !status.sealed {
        return Err(Error::Ceremony);
    }
    state.last_sequence = ceremony.sequence;
    state.last_challenge = Some(ceremony.challenge.clone());
    protected::save(&config.state_file, &state)?;
    let raw = protected::read(&config.share_file)?;
    let share = std::str::from_utf8(&raw)
        .map_err(|_| Error::ProtectedFile)?
        .trim();
    if share.len() < 16
        || share.len() > 4096
        || !share
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"+/=_-".contains(&c))
    {
        return Err(Error::ProtectedFile);
    }
    // Recheck after disk I/O. TLS and attestor bind this enrolled target; the
    // operator must fence rekey/restart while the identity lease is published.
    verified_status(&config.target).await?;
    ceremony.validate(&config.target, &config.roster, now())?;
    #[derive(Serialize)]
    struct Body<'a> {
        key: &'a str,
    }
    let body =
        Zeroizing::new(serde_json::to_vec(&Body { key: share }).map_err(|_| Error::ProtectedFile)?);
    let response = client(&config.target.endpoint)?
        .post(url(&config.target.endpoint, "/v1/sys/unseal")?)
        .header("content-type", "application/json")
        .body(bytes::Bytes::from_owner(body))
        .send()
        .await
        .map_err(|_| Error::Transport)?;
    let result: SealStatus = decode(response).await?;
    result.check(&config.target)?;
    Ok(())
}
