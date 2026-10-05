// SPDX-License-Identifier: AGPL-3.0-or-later
use super::{Configure, Mcp, Prepare, Status};
use crate::commands::auth_enrollment::{
    CompanyOrigin,
    credentials::{self, CredentialStore, NativeKeychain},
    transport::CompanyClient,
    workspace,
};
use anyhow::{Result, bail, ensure};
use chrono::Utc;
use mem_federation_runtime::{
    DeviceProjectSubscription, LocalProjectReplicaProfile, ProjectReplica,
};
use mem_mcp::laptop::LaptopCacheBackend;
use mem_record::{
    CompanyAgentApproveRequest, CompanyAgentReceipt, CompanyDeviceSelection, OnboardingArea,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

// Never Debug: separate credentials stay in Keychain, not journals/config/argv.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    company: String,
    device_id: Uuid,
    credential: String,
    request: CompanyAgentApproveRequest,
    bootstrap: DeviceProjectSubscription,
    receipt: Option<CompanyAgentReceipt>,
    observed_revoked: bool,
}
fn service(origin: &CompanyOrigin) -> String {
    format!(
        "net.memory-layer.company-agent:{}",
        credentials::sha256(&origin.0)
    )
}
fn fence(id: Uuid) -> String {
    format!("denied:{id}")
}
fn save(store: &impl CredentialStore, origin: &CompanyOrigin, r: &Record) -> Result<()> {
    // A separate monotonic item cannot be erased by a stale successful writer.
    // Ordinary status/prepare/save never delete this tombstone.
    if r.observed_revoked {
        store.put(&service(origin), &fence(r.request.agent_id), b"denied")?;
    }
    let mut merged = r.clone();
    if let Some(prior) = store.get(&service(origin), &r.request.agent_id.to_string())? {
        let prior: Record = serde_json::from_slice(&prior)
            .map_err(|_| anyhow::anyhow!("independent agent Keychain record changed"))?;
        ensure!(
            prior.request == r.request
                && prior.company == r.company
                && prior.credential == r.credential
                && prior.bootstrap == r.bootstrap,
            "refusing to replace independent identity or policy"
        );
        if merged.receipt.is_none() {
            merged.receipt = prior.receipt;
        }
    }
    merged.observed_revoked |= store
        .get(&service(origin), &fence(r.request.agent_id))?
        .is_some();
    store.put(
        &service(origin),
        &r.request.agent_id.to_string(),
        &serde_json::to_vec(&merged)?,
    )
}
fn load(store: &impl CredentialStore, origin: &CompanyOrigin, id: Uuid) -> Result<Record> {
    ensure!(!id.is_nil(), "exact independent agent ID required");
    let bytes = store
        .get(&service(origin), &id.to_string())?
        .ok_or_else(|| anyhow::anyhow!("independent agent credential missing from Keychain"))?;
    let mut r: Record = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("invalid independent agent Keychain record"))?;
    r.observed_revoked |= store.get(&service(origin), &fence(id))?.is_some();
    ensure!(
        r.company == origin.0
            && r.request.agent_id == id
            && r.device_id == r.bootstrap.device
            && !r.request.request_id.is_nil()
            && !r.request.principal_id.is_nil()
            && !r.request.token_id.is_nil()
            && r.credential.len() == 68
            && r.credential.starts_with("mlt_")
            && r.credential[4..].bytes().all(|c| c.is_ascii_hexdigit())
            && r.request.credential_sha256 == credentials::sha256(&r.credential)
            && r.request.projects.len() == 1
            && r.request.provider_id == r.request.caller.provider_id(),
        "independent agent Keychain binding changed"
    );
    let p = &r.request.projects[0];
    let scope = &r.bootstrap.scope;
    ensure!(
        p.offline_confirmed
            && p.area != OnboardingArea::Trust
            && p.area.as_str() == scope.area
            && p.owner_instance_id == scope.owner.as_uuid()
            && p.project_id == scope.project
            && u64::try_from(p.manifest_revision).ok() == Some(r.bootstrap.catalog_revision),
        "independent agent project scope changed"
    );
    r.bootstrap.validate()?;
    Ok(r)
}
fn eligible(r: &Record, receipt: &CompanyAgentReceipt) -> Result<()> {
    let q = &r.request;
    ensure!(
        !r.observed_revoked
            && receipt.schema_version == 1
            && receipt.agent_id == q.agent_id
            && receipt.request_id == q.request_id
            && receipt.device_id == r.device_id
            && receipt.company_id == r.bootstrap.scope.company
            && !receipt.human_id.is_nil()
            && !receipt.issuer.is_empty()
            && !receipt.subject.is_empty()
            && receipt.principal_id == q.principal_id
            && receipt.token_id == q.token_id
            && receipt.caller == q.caller
            && receipt.provider_id == q.provider_id
            && receipt.credential_sha256 == q.credential_sha256
            && receipt.expires_at == q.expires_at
            && receipt.projects.len() == 1
            && receipt.projects[0].selection == q.projects[0]
            && receipt.projects[0].destination.name == q.caller.as_str()
            && receipt.projects[0].destination.provider_id == q.provider_id
            && receipt.projects[0].destination.destination_id == q.provider_id
            && receipt.revoked_at.is_none()
            && receipt.expires_at > Utc::now()
            && receipt.approved_at <= Utc::now()
            && receipt.expires_at > receipt.approved_at
            && receipt.expires_at - receipt.approved_at <= chrono::Duration::hours(24),
        "disclosure receipt invalid, expired, revoked or changed"
    );
    if let Some(prior) = &r.receipt {
        ensure!(
            serde_json::to_value(prior)? == serde_json::to_value(receipt)?,
            "approved identity or disclosure policy changed; no automatic adoption"
        );
    }
    Ok(())
}
pub(super) async fn prepare(args: &Prepare) -> Result<()> {
    let origin = CompanyOrigin::parse(&args.company.company, args.company.allow_loopback)?;
    let store = NativeKeychain;
    let enrollment = workspace::active(&store, &origin)?;
    let (company, device, manifest) = workspace::agent_manifest(&args.workspace_journal)?;
    ensure!(
        company == origin.0 && device == enrollment.device_id,
        "workspace belongs to a different company/device"
    );
    let client = CompanyClient::new(origin.clone())?;
    let catalog = client.projects(&enrollment).await?;
    let matching = catalog
        .projects
        .iter()
        .find(|p| {
            p.manifest.project_id == manifest.project_id
                && p.manifest.entry.area == manifest.entry.area
                && p.manifest.entry.owner_instance_id == manifest.entry.owner_instance_id
        })
        .ok_or_else(|| anyhow::anyhow!("exact project no longer selected"))?;
    workspace::validate_project(matching)?;
    ensure!(
        serde_json::to_value(&manifest)? == serde_json::to_value(&matching.manifest)?,
        "manifest changed; renew project review first"
    );
    let caller = args.caller.contract();
    ensure!(
        manifest
            .entry
            .destinations
            .iter()
            .any(|d| d.name == caller.as_str()
                && d.provider_id == caller.provider_id()
                && d.destination_id == caller.provider_id()),
        "operator has not reviewed this caller/model disclosure destination"
    );
    let index = format!(
        "{}:{}:{}:{}:{}",
        device,
        caller.as_str(),
        manifest.entry.area.as_str(),
        manifest.entry.owner_instance_id,
        manifest.project_id
    );
    let saved = if let Some(index) = store.get(&service(&origin), &index)? {
        let id = std::str::from_utf8(&index)?.parse::<Uuid>()?;
        load(&store, &origin, id)?.into()
    } else {
        None
    };
    let r = if let Some(saved) = saved.filter(|_| !args.new_request) {
        saved
    } else {
        if args.new_request {
            let prior = store.get(&service(&origin), &index)?.ok_or_else(|| {
                anyhow::anyhow!("no prior request to replace; omit --new-request")
            })?;
            let id = std::str::from_utf8(&prior)?.parse::<Uuid>()?;
            let prior = load(&store, &origin, id)?;
            ensure!(
                prior.observed_revoked || prior.request.expires_at <= Utc::now(),
                "prior request is still usable; revoke it in the browser before replacing it"
            );
        }
        let bootstrap = client
            .subscription(
                &enrollment,
                manifest.project_id,
                manifest.entry.area.as_str(),
                manifest.entry.owner_instance_id,
            )
            .await?;
        bootstrap.validate()?;
        ensure!(
            bootstrap.device == device
                && bootstrap.scope.project == manifest.project_id
                && bootstrap.scope.area == manifest.entry.area.as_str()
                && bootstrap.scope.owner.as_uuid() == manifest.entry.owner_instance_id
                && u64::try_from(manifest.revision).ok() == Some(bootstrap.catalog_revision),
            "selected control-plane bootstrap mismatch"
        );
        let credential = format!("mlt_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let r = Record {
            company: origin.0.clone(),
            device_id: device,
            request: CompanyAgentApproveRequest {
                request_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                principal_id: Uuid::new_v4(),
                token_id: Uuid::new_v4(),
                caller,
                provider_id: caller.provider_id().into(),
                credential_sha256: credentials::sha256(&credential),
                expires_at: Utc::now() + chrono::Duration::hours(24),
                confirmed: true,
                projects: vec![CompanyDeviceSelection {
                    area: manifest.entry.area,
                    owner_instance_id: manifest.entry.owner_instance_id,
                    project_id: manifest.project_id,
                    manifest_revision: manifest.revision,
                    manifest_sha256: mem_federation_runtime::onboarding_manifest_digest(&manifest)?,
                    offline_confirmed: true,
                }],
            },
            credential,
            bootstrap,
            receipt: None,
            observed_revoked: false,
        };
        save(&store, &origin, &r)?;
        store.put(
            &service(&origin),
            &index,
            r.request.agent_id.to_string().as_bytes(),
        )?;
        r
    };
    ensure!(
        r.request.caller == caller
            && r.request.projects[0].manifest_revision == manifest.revision
            && r.request.projects[0].manifest_sha256
                == mem_federation_runtime::onboarding_manifest_digest(&manifest)?,
        "saved agent request binds another manifest; no automatic renewal"
    );
    ensure!(
        !r.observed_revoked && r.request.expires_at > Utc::now(),
        "saved agent request expired or was revoked; no automatic renewal"
    );
    let envelope = public_envelope(&r);
    println!("{}", serde_json::to_string_pretty(&envelope)?);
    if args.open_browser {
        let url = approval_url(&origin, &envelope)?;
        crate::commands::auth_enrollment::transport::open_browser(url.as_str()).map_err(|_| {
            anyhow::anyhow!(
                "browser could not open; import the printed public approval JSON manually"
            )
        })?;
    }
    Ok(())
}
fn public_envelope(r: &Record) -> Value {
    let mut request = r.request.clone();
    request.confirmed = false;
    json!({"device_id":r.device_id,"approval_endpoint":format!("/v1/onboarding/devices/{}/agents",r.device_id),"request":request,"agent_id":r.request.agent_id,"contains_secret":false,"automatic_approval":false})
}
fn approval_url(origin: &CompanyOrigin, envelope: &Value) -> Result<reqwest::Url> {
    let body = serde_json::to_string(envelope)?;
    ensure!(
        body.len() <= 8192,
        "public approval envelope exceeds browser limit; import JSON manually"
    );
    let mut url = reqwest::Url::parse(&origin.endpoint("/"))?;
    url.query_pairs_mut().append_pair("agent_request", &body);
    ensure!(
        url.as_str().len() <= 8192 && url.origin().ascii_serialization() == origin.0,
        "public approval URL exceeds bound or origin changed; import JSON manually"
    );
    Ok(url)
}
enum Observation {
    Current(Box<CompanyAgentReceipt>),
    Unreachable,
    Denied,
}
fn persist_observation(
    store: &impl CredentialStore,
    origin: &CompanyOrigin,
    id: Uuid,
    observation: Observation,
    allow_offline: bool,
) -> Result<(Record, bool)> {
    // Reload after HTTP. Another process may have activated or revoked the
    // grant while this request was in flight; never merge an obsolete view.
    let mut r = load(store, origin, id)?;
    let result = accept(&mut r, observation, allow_offline);
    save(store, origin, &r)?;
    let current = result?;
    r = load(store, origin, id)?;
    eligible(
        &r,
        r.receipt
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("online activation required"))?,
    )?;
    Ok((r, current))
}
fn finish_authorization(
    store: &impl CredentialStore,
    origin: &CompanyOrigin,
    id: Uuid,
    observation: Observation,
    allow_offline: bool,
) -> Result<(Record, bool)> {
    let (r, current) = persist_observation(store, origin, id, observation, allow_offline)?;
    // This check happens after the last HTTP await, including the final check
    // after an awaited database read. Pending logout is not offline consent.
    let parent = workspace::active(store, origin)?;
    ensure!(
        parent.device_id == r.device_id,
        "parent changed during agent authorization"
    );
    Ok((r, current))
}
fn accept(r: &mut Record, observation: Observation, allow_offline: bool) -> Result<bool> {
    match observation {
        Observation::Current(receipt) => {
            eligible(r, &receipt)?;
            r.receipt = Some(*receipt);
            Ok(true)
        }
        Observation::Denied => {
            // Even initial activation may have an older successful receipt in
            // flight. Every definitive denial therefore fences this ID forever.
            // Renew only through an explicit new request and human approval.
            r.observed_revoked = true;
            bail!(
                "agent denied or not approved; this credential is fenced. Approve a fresh prepare --new-request; no cached disclosure authorized"
            );
        }
        Observation::Unreachable => {
            ensure!(
                allow_offline,
                "company unreachable and offline model disclosure not enabled"
            );
            eligible(
                r,
                r.receipt.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("online activation required before cached model disclosure")
                })?,
            )?;
            Ok(false)
        }
    }
}
async fn observe(origin: &CompanyOrigin, r: &Record) -> Result<Observation> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()?;
    let mut response = match client
        .get(origin.endpoint(&format!(
            "/v1/onboarding/agents/{}/receipt",
            r.request.agent_id
        )))
        .bearer_auth(&r.credential)
        .send()
        .await
    {
        Ok(v) => v,
        Err(_) => return Ok(Observation::Unreachable),
    };
    if matches!(response.status().as_u16(), 401 | 403 | 404 | 409 | 410) {
        return Ok(Observation::Denied);
    }
    ensure!(
        response.status().is_success(),
        "company agent verification failed; details suppressed"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("agent receipt interrupted; no offline fallback"))?
    {
        ensure!(
            bytes.len() + chunk.len() <= 64 * 1024,
            "agent receipt exceeds bounds"
        );
        bytes.extend_from_slice(&chunk);
    }
    let receipt = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("invalid company agent receipt; no offline fallback"))?;
    if eligible(r, &receipt).is_err() {
        return Ok(Observation::Denied);
    }
    Ok(Observation::Current(Box::new(receipt)))
}
pub(super) async fn status(args: &Status) -> Result<()> {
    let origin = CompanyOrigin::parse(&args.company.company, args.company.allow_loopback)?;
    let r = load(&NativeKeychain, &origin, args.agent_id)?;
    let observation = observe(&origin, &r).await?;
    let (r, _) = persist_observation(&NativeKeychain, &origin, args.agent_id, observation, false)?;
    println!("{}", serde_json::to_string_pretty(&r.receipt)?);
    Ok(())
}

fn configuration_inputs(
    args: &Configure,
    origin: &CompanyOrigin,
    r: &Record,
    profile: &LocalProjectReplicaProfile,
) -> Result<super::settings::Inputs> {
    let (company, device, workspace, manifest, journal_sha) =
        workspace::agent_settings_workspace(&args.workspace_journal, &args.profile)?;
    let parent = workspace::active(&NativeKeychain, origin)?;
    let latest = load(&NativeKeychain, origin, args.agent_id)?;
    ensure!(
        company == origin.0
            && device == r.device_id
            && parent.device_id == device
            && !latest.observed_revoked
            && latest.request == r.request
            && latest.bootstrap == r.bootstrap,
        "reviewed workspace or parent agent binding changed"
    );
    let receipt = latest
        .receipt
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("online approved agent receipt required"))?;
    eligible(&latest, receipt)?;
    let current_profile = LocalProjectReplicaProfile::read(&args.profile)?;
    ensure!(
        serde_json::to_value(&current_profile)? == serde_json::to_value(profile)?
            && profile.company == company
            && profile.bootstrap == r.bootstrap,
        "native profile changed before settings configuration"
    );
    let selected = &r.request.projects[0];
    ensure!(
        manifest.entry.offline_allowed
            && manifest.entry.area != OnboardingArea::Trust
            && manifest.entry.area == selected.area
            && manifest.entry.owner_instance_id == selected.owner_instance_id
            && manifest.project_id == selected.project_id
            && manifest.revision == selected.manifest_revision
            && mem_federation_runtime::onboarding_manifest_digest(&manifest)?
                == selected.manifest_sha256,
        "completed workspace manifest is not the exact approved disclosure scope"
    );
    ensure!(
        manifest
            .entry
            .destinations
            .iter()
            .any(|d| d.name == r.request.caller.as_str()
                && d.provider_id == r.request.provider_id
                && d.destination_id == r.request.provider_id),
        "reviewed manifest does not permit exact agent destination"
    );
    let command =
        std::env::current_exe().map_err(|_| anyhow::anyhow!("installed executable unavailable"))?;
    let command = command
        .to_str()
        .filter(|v| command.is_absolute() && !v.chars().any(char::is_control))
        .ok_or_else(|| anyhow::anyhow!("absolute installed executable required"))?
        .to_owned();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::symlink_metadata(&command)
            .map_err(|_| anyhow::anyhow!("installed executable unavailable"))?;
        ensure!(
            m.is_file()
                && (m.uid() == 0 || m.uid() == rustix::process::getuid().as_raw())
                && m.mode() & 0o022 == 0
                && m.mode() & 0o111 != 0,
            "installed executable must be trusted and non-shared-writable"
        );
    }
    let hash =
        |v: &Value| -> Result<String> { Ok(credentials::sha256(&serde_json::to_string(v)?)) };
    Ok(super::settings::Inputs {
        company,
        agent_id: args.agent_id,
        caller: r.request.caller,
        provider_id: r.request.provider_id.clone(),
        device_id: device,
        scope: serde_json::to_value(&r.bootstrap.scope)?,
        workspace,
        profile: args.profile.clone(),
        workspace_journal_sha256: journal_sha,
        profile_sha256: hash(&serde_json::to_value(profile)?)?,
        bootstrap_sha256: hash(&serde_json::to_value(&r.bootstrap)?)?,
        manifest_sha256: selected.manifest_sha256.clone(),
        receipt_sha256: hash(&serde_json::to_value(receipt)?)?,
        command,
        allow_loopback: args.company.allow_loopback,
        allow_offline: args.allow_offline,
    })
}
pub(super) async fn configure(args: &Configure) -> Result<()> {
    let origin = CompanyOrigin::parse(&args.company.company, args.company.allow_loopback)?;
    let broker = CacheBroker {
        company: origin.0.clone(),
        allow_loopback: args.company.allow_loopback,
        agent_id: args.agent_id,
        profile: args.profile.clone(),
        allow_offline: false,
    };
    let (r, profile, online) = broker.authorize().await?;
    ensure!(
        online,
        "settings configuration requires current online approval"
    );
    let inputs = configuration_inputs(args, &origin, &r, &profile)?;
    let prepared = super::settings::prepare(
        &NativeKeychain,
        &origin,
        inputs,
        &args.workspace_journal,
        args.plan_sha256.as_deref(),
    )?;
    let output = if args.confirm {
        ensure!(
            broker.recheck(&r, &profile).await?,
            "settings apply requires current online approval"
        );
        ensure!(
            configuration_inputs(args, &origin, &r, &profile)? == *prepared.inputs(),
            "reviewed inputs changed before settings apply"
        );
        prepared.apply(&NativeKeychain, &origin)?
    } else {
        prepared.preview(false, false)
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[derive(Clone)]
struct CacheBroker {
    company: String,
    allow_loopback: bool,
    agent_id: Uuid,
    profile: PathBuf,
    allow_offline: bool,
}
impl CacheBroker {
    async fn authorize(&self) -> Result<(Record, LocalProjectReplicaProfile, bool)> {
        let origin = CompanyOrigin::parse(&self.company, self.allow_loopback)?;
        let r = load(&NativeKeychain, &origin, self.agent_id)?;
        let parent = workspace::active(&NativeKeychain, &origin)?;
        ensure!(
            parent.device_id == r.device_id && !r.observed_revoked,
            "parent logout/revocation blocks model disclosure"
        );
        let observed = observe(&origin, &r).await?;
        let (r, current) = finish_authorization(
            &NativeKeychain,
            &origin,
            self.agent_id,
            observed,
            self.allow_offline,
        )?;
        let profile = LocalProjectReplicaProfile::read(&self.profile)?;
        ensure!(
            profile.company == r.company && profile.bootstrap == r.bootstrap,
            "native cache profile does not match the exact approved project/trust"
        );
        Ok((r, profile, current))
    }
    async fn recheck(&self, prior: &Record, profile: &LocalProjectReplicaProfile) -> Result<bool> {
        let (current, confirmed, online) = self.authorize().await?;
        ensure!(
            current.request == prior.request
                && current.bootstrap == prior.bootstrap
                && confirmed.bootstrap == profile.bootstrap
                && confirmed.subscription == profile.subscription
                && confirmed.local_database_url == profile.local_database_url,
            "agent grant/profile changed during cache read"
        );
        Ok(online)
    }
}
impl LaptopCacheBackend for CacheBroker {
    async fn status(&self) -> Result<Value> {
        let (r, profile, _) = self.authorize().await?;
        let status = ProjectReplica::connect(&profile, false)
            .await?
            .status()
            .await?;
        let current = self.recheck(&r, &profile).await?;
        Ok(
            json!({"read_only":true,"scope":profile.bootstrap.scope,"subscription":profile.subscription,"cursor":status.cursor,"last_committed_page":status.observed_at,"current_remote_access_verified":current,"disclosure_expires_at":r.request.expires_at,"caller":r.request.caller,"provider_id":r.request.provider_id}),
        )
    }
    async fn search(&self, question: String, limit: u32) -> Result<Value> {
        let (r, profile, _) = self.authorize().await?;
        let hits = ProjectReplica::connect(&profile, false)
            .await?
            .search(&question, limit)
            .await?;
        // Both remote and local observed revocation fence awaited reads.
        let current = self.recheck(&r, &profile).await?;
        Ok(
            json!({"read_only":true,"scope":profile.bootstrap.scope,"subscription":profile.subscription,"current_remote_access_verified":current,"disclosure_expires_at":r.request.expires_at,"results":hits}),
        )
    }
}
pub(super) async fn mcp(args: &Mcp) -> Result<()> {
    let broker = CacheBroker {
        company: args.company.company.clone(),
        allow_loopback: args.company.allow_loopback,
        agent_id: args.agent_id,
        profile: args.profile.clone(),
        allow_offline: args.allow_offline,
    };
    broker.authorize().await?; // Never print metadata/credentials on the stdio protocol.
    mem_mcp::laptop::run_laptop_stdio(broker).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use mem_record::{CompanyAgentCaller, CompanyAgentScope, OnboardingDestination};
    use std::{collections::BTreeMap, sync::Mutex};
    #[test]
    fn browser_approval_contains_only_public_request_and_exact_origin() {
        let (origin, r, _) = fixture();
        let envelope = public_envelope(&r);
        let raw = serde_json::to_string(&envelope).unwrap();
        assert!(!raw.contains(&r.credential));
        assert_eq!(envelope["request"]["confirmed"], false);
        assert_eq!(envelope["contains_secret"], false);
        let url = approval_url(&origin, &envelope).unwrap();
        assert_eq!(url.origin().ascii_serialization(), origin.0);
        assert_eq!(url.path(), "/");
        let pairs: Vec<_> = url.query_pairs().collect();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "agent_request");
        assert_eq!(
            serde_json::from_str::<Value>(&pairs[0].1).unwrap(),
            envelope
        );
        assert!(approval_url(&origin, &json!({"oversized":"x".repeat(8192)})).is_err());
    }
    #[derive(Default)]
    struct Store(Mutex<BTreeMap<(String, String), Vec<u8>>>);
    impl CredentialStore for Store {
        fn get(&self, s: &str, a: &str) -> Result<Option<Vec<u8>>> {
            Ok(self.0.lock().unwrap().get(&(s.into(), a.into())).cloned())
        }
        fn put(&self, s: &str, a: &str, v: &[u8]) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .insert((s.into(), a.into()), v.to_vec());
            Ok(())
        }
        fn delete(&self, s: &str, a: &str) -> Result<()> {
            self.0.lock().unwrap().remove(&(s.into(), a.into()));
            Ok(())
        }
    }
    fn fixture() -> (CompanyOrigin, Record, CompanyAgentReceipt) {
        let origin = CompanyOrigin::parse("https://company.example.invalid", false).unwrap();
        let device = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let project = Uuid::new_v4();
        let company = Uuid::new_v4();
        let security =
            mem_federation_runtime::GatewaySecurity::initial(&[41; 32], Default::default())
                .unwrap();
        let bootstrap=serde_json::from_value(json!({"schema_version":1,"device":device,"catalog_revision":1,"authority":Uuid::new_v4(),"receipt_keys":security.trust().keys(),"scope":{"company":company,"area":"workshop","owner":owner,"project":project,"space":Uuid::new_v4()}})).unwrap();
        let credential = format!("mlt_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let request = CompanyAgentApproveRequest {
            request_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            principal_id: Uuid::new_v4(),
            token_id: Uuid::new_v4(),
            caller: CompanyAgentCaller::Codex,
            provider_id: "openai".into(),
            credential_sha256: credentials::sha256(&credential),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            confirmed: true,
            projects: vec![CompanyDeviceSelection {
                area: OnboardingArea::Workshop,
                owner_instance_id: owner,
                project_id: project,
                manifest_revision: 1,
                manifest_sha256: "a".repeat(64),
                offline_confirmed: true,
            }],
        };
        let receipt = CompanyAgentReceipt {
            schema_version: 1,
            request_id: request.request_id,
            agent_id: request.agent_id,
            device_id: device,
            human_id: Uuid::new_v4(),
            issuer: "https://idp.example.invalid".into(),
            subject: "stable-user".into(),
            company_id: company,
            principal_id: request.principal_id,
            token_id: request.token_id,
            caller: request.caller,
            provider_id: request.provider_id.clone(),
            credential_sha256: request.credential_sha256.clone(),
            expires_at: request.expires_at,
            projects: vec![CompanyAgentScope {
                selection: request.projects[0].clone(),
                destination: OnboardingDestination {
                    name: "codex".into(),
                    provider_id: "openai".into(),
                    destination_id: "openai".into(),
                },
            }],
            approved_at: Utc::now(),
            revoked_at: None,
        };
        let r = Record {
            company: origin.0.clone(),
            device_id: device,
            credential,
            request,
            bootstrap,
            receipt: None,
            observed_revoked: false,
        };
        (origin, r, receipt)
    }
    #[test]
    fn independent_keychain_records_are_not_device_or_sibling_credentials() {
        let (origin, r, _) = fixture();
        let store = Store::default();
        save(&store, &origin, &r).unwrap();
        assert_eq!(
            load(&store, &origin, r.request.agent_id)
                .unwrap()
                .credential,
            r.credential
        );
        assert!(load(&store, &origin, Uuid::new_v4()).is_err());
        assert!(
            load(
                &store,
                &CompanyOrigin::parse("https://other.example.invalid", false).unwrap(),
                r.request.agent_id
            )
            .is_err()
        );
        assert!(
            !serde_json::to_string(&r.request)
                .unwrap()
                .contains(&r.credential)
        );
        let mut bad = r.clone();
        bad.credential = "mlt_".to_owned() + &"b".repeat(64);
        assert!(save(&store, &origin, &bad).is_err());
        store
            .put(
                &service(&origin),
                &r.request.agent_id.to_string(),
                &serde_json::to_vec(&bad).unwrap(),
            )
            .unwrap();
        assert!(load(&store, &origin, r.request.agent_id).is_err());
    }
    #[test]
    fn receipts_reject_sibling_destination_scope_and_policy_replacement() {
        let (_, r, receipt) = fixture();
        eligible(&r, &receipt).unwrap();
        for change in [
            |r: &mut CompanyAgentReceipt| r.agent_id = Uuid::new_v4(),
            |r: &mut CompanyAgentReceipt| r.token_id = Uuid::new_v4(),
            |r: &mut CompanyAgentReceipt| r.device_id = Uuid::new_v4(),
            |r: &mut CompanyAgentReceipt| r.company_id = Uuid::new_v4(),
            |r: &mut CompanyAgentReceipt| r.provider_id = "anthropic".into(),
            |r: &mut CompanyAgentReceipt| {
                r.projects[0].destination.destination_id = "local_human".into()
            },
            |r: &mut CompanyAgentReceipt| r.projects[0].selection.area = OnboardingArea::Trust,
            |r: &mut CompanyAgentReceipt| {
                r.projects[0].selection.owner_instance_id = Uuid::new_v4()
            },
            |r: &mut CompanyAgentReceipt| r.projects[0].selection.manifest_revision += 1,
            |r: &mut CompanyAgentReceipt| r.projects[0].selection.manifest_sha256 = "b".repeat(64),
            |r: &mut CompanyAgentReceipt| r.revoked_at = Some(Utc::now()),
        ] {
            let mut bad = receipt.clone();
            change(&mut bad);
            assert!(eligible(&r, &bad).is_err());
        }
        let mut pinned = r.clone();
        pinned.receipt = Some(receipt.clone());
        let mut changed = receipt;
        changed.human_id = Uuid::new_v4();
        assert!(eligible(&pinned, &changed).is_err());
    }
    #[test]
    fn offline_authorization_requires_explicit_consent_activation_and_unexpired_grant() {
        let (_, mut r, receipt) = fixture();
        assert!(accept(&mut r, Observation::Unreachable, true).is_err());
        assert!(
            accept(
                &mut r,
                Observation::Current(Box::new(receipt.clone())),
                false
            )
            .unwrap()
        );
        assert!(accept(&mut r, Observation::Unreachable, false).is_err());
        assert!(!accept(&mut r, Observation::Unreachable, true).unwrap());
        let mut expired = r.clone();
        expired.request.expires_at = Utc::now() - chrono::Duration::seconds(1);
        expired.receipt.as_mut().unwrap().expires_at = expired.request.expires_at;
        assert!(accept(&mut expired, Observation::Unreachable, true).is_err());
        assert!(accept(&mut r, Observation::Denied, true).is_err());
        assert!(r.observed_revoked);
        assert!(accept(&mut r, Observation::Unreachable, true).is_err());
        assert!(accept(&mut r, Observation::Current(Box::new(receipt)), false).is_err());
    }
    #[test]
    fn stale_success_cannot_erase_another_process_denial_or_reenable_offline_disclosure() {
        let (origin, mut stale, receipt) = fixture();
        let store = Store::default();
        stale.receipt = Some(receipt.clone());
        save(&store, &origin, &stale).unwrap();
        let id = stale.request.agent_id;
        assert!(persist_observation(&store, &origin, id, Observation::Denied, true).is_err());
        // A successful HTTP request started before the denial writes later.
        save(&store, &origin, &stale).unwrap();
        let mut final_record = load(&store, &origin, id).unwrap();
        assert!(final_record.observed_revoked);
        assert!(accept(&mut final_record, Observation::Unreachable, true).is_err());
        assert!(
            persist_observation(
                &store,
                &origin,
                id,
                Observation::Current(Box::new(receipt)),
                false
            )
            .is_err()
        );
    }
    #[test]
    fn denial_reloads_activation_completed_by_another_process() {
        let (origin, r, receipt) = fixture();
        let store = Store::default();
        let id = r.request.agent_id;
        save(&store, &origin, &r).unwrap();
        // A denial request starts while pending. A sibling activates the same
        // immutable grant before the denial arrives; it must now fence it.
        persist_observation(
            &store,
            &origin,
            id,
            Observation::Current(Box::new(receipt)),
            false,
        )
        .unwrap();
        assert!(persist_observation(&store, &origin, id, Observation::Denied, true).is_err());
        assert!(load(&store, &origin, id).unwrap().observed_revoked);
        assert!(persist_observation(&store, &origin, id, Observation::Unreachable, true).is_err());
    }
    #[test]
    fn initial_denial_fences_delayed_first_activation_but_not_a_fresh_identity() {
        let (origin, r, receipt) = fixture();
        let store = Store::default();
        let id = r.request.agent_id;
        save(&store, &origin, &r).unwrap();
        assert!(persist_observation(&store, &origin, id, Observation::Denied, false).is_err());
        save(&store, &origin, &r).unwrap(); // delayed preapproval writer
        assert!(
            persist_observation(
                &store,
                &origin,
                id,
                Observation::Current(Box::new(receipt)),
                false
            )
            .is_err()
        );
        assert!(persist_observation(&store, &origin, id, Observation::Unreachable, true).is_err());
        let (_, mut fresh, receipt) = fixture();
        fresh.company = origin.0.clone();
        save(&store, &origin, &fresh).unwrap();
        persist_observation(
            &store,
            &origin,
            fresh.request.agent_id,
            Observation::Current(Box::new(receipt)),
            false,
        )
        .unwrap();
        assert!(load(&store, &origin, id).unwrap().observed_revoked);
    }
    #[tokio::test]
    async fn logout_during_receipt_request_blocks_the_post_await_disclosure_check() {
        use std::sync::Arc;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (_, mut r, receipt) = fixture();
        let store = Arc::new(Store::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin =
            CompanyOrigin::parse(&format!("http://{}", listener.local_addr().unwrap()), true)
                .unwrap();
        r.company = origin.0.clone();
        save(store.as_ref(), &origin, &r).unwrap();
        let mut parent =
            credentials::load_or_create(store.as_ref(), &origin, "test laptop").unwrap();
        parent.device_id = r.device_id;
        parent.approved = true;
        credentials::save(store.as_ref(), &origin, &parent).unwrap();
        let parent_service = format!(
            "net.memory-layer.company-device:{}",
            credentials::sha256(&origin.0)
        );
        store
            .put(
                &parent_service,
                "active-device",
                r.device_id.to_string().as_bytes(),
            )
            .unwrap();
        let task_store = store.clone();
        let task_origin = origin.clone();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0; 8192];
            let received = socket.read(&mut buf).await.unwrap();
            assert!(
                received > 0,
                "authorization request must arrive before logout"
            );
            parent.pending_logout = true;
            credentials::save(task_store.as_ref(), &task_origin, &parent).unwrap();
            let body = serde_json::to_vec(&receipt).unwrap();
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(header.as_bytes()).await.unwrap();
            socket.write_all(&body).await.unwrap();
        });
        let observed = observe(&origin, &r).await.unwrap();
        task.await.unwrap();
        assert!(
            finish_authorization(store.as_ref(), &origin, r.request.agent_id, observed, true)
                .is_err()
        );
    }
    #[tokio::test]
    async fn own_agent_receipt_http_never_sends_the_parent_secret_or_follows_redirect() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (_, mut r, _) = fixture();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin =
            CompanyOrigin::parse(&format!("http://{}", listener.local_addr().unwrap()), true)
                .unwrap();
        r.company = origin.0.clone();
        let id = r.request.agent_id;
        let token = r.credential.clone();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0; 8192];
            let n = socket.read(&mut buffer).await.unwrap();
            let raw = String::from_utf8(buffer[..n].to_vec()).unwrap();
            socket.write_all(b"HTTP/1.1 302 Found\r\nLocation: https://untrusted.example.invalid\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            raw
        });
        assert!(observe(&origin, &r).await.is_err());
        let raw = task.await.unwrap();
        assert!(raw.contains(&format!("/v1/onboarding/agents/{id}/receipt")));
        assert!(
            raw.to_lowercase()
                .contains(&format!("authorization: bearer {token}"))
        );
        assert!(!raw.contains("x-api-token"));
        assert!(!raw.contains("cookie:"));
    }
}
