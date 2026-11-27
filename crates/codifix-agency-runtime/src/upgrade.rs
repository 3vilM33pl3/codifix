//! An external supervisor installs the tested artifact and observes the new process.
use crate::{
    config::{Config, validate_endpoint},
    discovery::bounded,
    social_transport::{Call, Transport},
    store::{Store, StoredTask, enqueue, fingerprint},
};
use anyhow::{Context, Result, ensure};
use codifix_agency::Usage;
use codifix_store::effects::{self, EffectIntent, EffectKind, EffectReason, EffectStatus};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
use url::Origin;
use uuid::Uuid;

#[derive(Deserialize, Serialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
struct Release {
    version: String,
    url: String,
    sha256: String,
    artifact_test_sha256: String,
    upgrade_test_sha256: String,
}
const WORKER_INSTALLATION: &str = "installation";
const TUI_INSTALLATION: &str = "tui_installation";

impl Release {
    fn validate(&self) -> Result<()> {
        validate_endpoint(&self.url, true)?;
        ensure!(
            !self.version.is_empty() && self.version.len() <= 64,
            "invalid version"
        );
        for digest in [
            &self.sha256,
            &self.artifact_test_sha256,
            &self.upgrade_test_sha256,
        ] {
            ensure!(
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid release evidence digest"
            );
        }
        Ok(())
    }
}

pub(crate) async fn request(
    store: &Store,
    cfg: &Config,
    task: &StoredTask,
) -> Result<Option<Value>> {
    let tui = task.request.resource.kind == TUI_INSTALLATION;
    ensure!(
        (tui || task.request.resource.kind == WORKER_INSTALLATION)
            && task.request.resource.id == cfg.instance_id.to_string(),
        "installation scope mismatch"
    );
    let release: Release = serde_json::from_value(task.request.payload["release"].clone())?;
    release.validate()?;
    if tui {
        tui_object(&release.url, &release.version)?;
        require_private_peer(cfg, &task.requester)?;
    }
    let id = task.contract.id;
    let value = serde_json::to_value(&release)?;
    sqlx::query("INSERT INTO agency_upgrades(task_id,instance_id,release,state) VALUES($1,$2,$3,'queued') ON CONFLICT DO NOTHING")
        .bind(id).bind(cfg.instance_id).bind(&value).execute(&store.pool).await?;
    let (saved, state, report): (Value, String, Option<Value>) =
        sqlx::query_as("SELECT release,state,report FROM agency_upgrades WHERE task_id=$1")
            .bind(id)
            .fetch_one(&store.pool)
            .await?;
    ensure!(saved == value, "upgrade request changed");
    if state == "installed" {
        let mut report = report.context("missing installation report")?;
        let failed_step = if tui && report["companion_footer_verified"] != true {
            Some("tui_footer")
        } else if !tui && release.version != env!("CARGO_PKG_VERSION") {
            Some("running_version")
        } else if !tui
            && !running_digest()
                .await
                .is_ok_and(|digest| digest == release.sha256)
        {
            Some("running_digest")
        } else if verify_roundtrip(store, cfg, task, &release.version)
            .await
            .is_err()
        {
            Some("post_restart_peer_message")
        } else {
            None
        };
        let (state, result) = if let Some(step) = failed_step {
            ("failed", json!({"step":step,"assistance_requested":true}))
        } else {
            report["peer_message"] = json!(true);
            if !tui {
                report["running_sha256"] = json!(release.sha256);
            }
            ("verified", report)
        };
        sqlx::query(
            "UPDATE agency_upgrades SET state=$2,report=$3 WHERE task_id=$1 AND state='installed'",
        )
        .bind(id)
        .bind(state)
        .bind(&result)
        .execute(&store.pool)
        .await?;
        if let Some(step) = failed_step {
            return wait_for_help(store, task, step).await;
        }
        return Ok(Some(result));
    }
    match state.as_str() {
        "verified" => Ok(Some(report.context("missing verification")?)),
        "failed" => {
            let step = report
                .as_ref()
                .and_then(|r| r["step"].as_str())
                .unwrap_or("verification");
            wait_for_help(store, task, step).await
        }
        _ => Ok(None),
    }
}

async fn wait_for_help(store: &Store, task: &StoredTask, step: &str) -> Result<Option<Value>> {
    let mut task = task.clone();
    task.contract.clarify(
        format!("Upgrade stopped during {step}. Codifix Agent assistance is required."),
        chrono::Utc::now(),
    )?;
    store.update_contract(&task).await?;
    Ok(None)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TuiProfile {
    object_origin: String,
    fetch_command: Vec<String>,
    verify_command: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    publisher_peer: String,
    release_receipt: Option<PathBuf>,
    artifact: PathBuf,
    installed_artifact: PathBuf,
    install_command: Vec<String>,
    companion_health: Option<String>,
    openbao_health: Option<String>,
    tui: Option<TuiProfile>,
}

fn secure_object_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).context("release_gate")?;
    ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && value == url.as_str(),
        "release_gate"
    );
    Ok(url)
}

fn trusted_object_origin(value: &str) -> Result<Origin> {
    let url = secure_object_url(value)?;
    ensure!(url.path() == "/", "release_gate");
    Ok(url.origin())
}

fn tui_object(value: &str, expected_version: &str) -> Result<Url> {
    let url = secure_object_url(value)?;
    let path = url.path();
    let (version, file) = path
        .strip_prefix("/codifix-releases/ca/")
        .and_then(|suffix| suffix.split_once('/'))
        .context("release_gate")?;
    ensure!(
        !version.is_empty()
            && version == expected_version
            && version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
            && file.starts_with("tui-companion-footer-")
            && file
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b)),
        "release_gate"
    );
    Ok(url)
}

fn tui_object_key(value: &str, expected_version: &str, origin: &Origin) -> Result<String> {
    let url = tui_object(value, expected_version)?;
    ensure!(&url.origin() == origin, "release_gate");
    Ok(url.path().trim_start_matches('/').to_owned())
}

fn require_private_peer(cfg: &Config, id: &str) -> Result<()> {
    ensure!(
        cfg.social
            .as_ref()
            .is_some_and(|s| s.peers.contains_key(id)),
        "private XRPC required"
    );
    Ok(())
}

async fn local_command(
    command: &[String],
    arguments: &[&std::ffi::OsStr],
    step: &'static str,
) -> Result<()> {
    let (program, args) = command.split_first().context(step)?;
    ensure!(Path::new(program).is_absolute(), step);
    let mut child = Command::new(program);
    child.args(args).args(arguments).kill_on_drop(true);
    child
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let status = tokio::time::timeout(Duration::from_secs(300), child.status())
        .await
        .context(step)?
        .context(step)?;
    ensure!(status.success(), step);
    Ok(())
}

pub async fn supervise(store: &Store, cfg: &Config, path: &Path) -> Result<bool> {
    let profile: Profile = serde_json::from_slice(&std::fs::read(path)?)?;
    let object_origin = profile
        .tui
        .as_ref()
        .map(|tui| trusted_object_origin(&tui.object_origin))
        .transpose()?;
    let (kind, pinned, health) = if profile.tui.is_some() {
        require_private_peer(cfg, &profile.publisher_peer)?;
        ensure!(
            profile.artifact.is_absolute()
                && profile.installed_artifact.is_absolute()
                && profile.artifact != profile.installed_artifact,
            "invalid TUI staging and installation paths"
        );
        (TUI_INSTALLATION, None, None)
    } else {
        let receipt = profile
            .release_receipt
            .as_ref()
            .context("missing release receipt")?;
        let release: Release = serde_json::from_slice(&std::fs::read(receipt)?)?;
        release.validate()?;
        let health = (
            profile
                .companion_health
                .as_deref()
                .context("missing companion health endpoint")?,
            profile
                .openbao_health
                .as_deref()
                .context("missing OpenBao health endpoint")?,
        );
        (WORKER_INSTALLATION, Some(release), Some(health))
    };
    let mut tx = store.pool.begin().await?;
    // Serialize intake on this CA even if two timer invocations overlap.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("upgrade:{}", cfg.instance_id))
        .execute(&mut *tx)
        .await?;
    let next: Option<(Uuid, Value)> = sqlx::query_as("SELECT u.task_id,u.release FROM agency_upgrades u JOIN agency_tasks t ON t.id=u.task_id WHERE u.instance_id=$1 AND u.state='queued' AND t.payload#>>'{resource,kind}'=$2 AND NOT EXISTS (SELECT 1 FROM agency_upgrades busy JOIN agency_tasks bt ON bt.id=busy.task_id WHERE busy.instance_id=$1 AND busy.state IN ('installing','installed') AND bt.payload#>>'{resource,kind}'=$2) ORDER BY u.created_at LIMIT 1 FOR UPDATE OF u SKIP LOCKED")
        .bind(cfg.instance_id).bind(kind).fetch_optional(&mut *tx).await?;
    let Some((id, requested)) = next else {
        return Ok(false);
    };
    sqlx::query("UPDATE agency_upgrades SET state='installing' WHERE task_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let task = store.get_internal(id).await?;
    let intent_id = Uuid::new_v5(&id, b"upgrade");
    let result = async {
        ensure!(task.requester == profile.publisher_peer, "release_gate");
        let release: Release = serde_json::from_value(requested.clone()).context("release_gate")?;
        release.validate().context("release_gate")?;
        if let Some(pinned) = &pinned {
            ensure!(*pinned == release, "release_gate");
        }
        if let Some(tui) = &profile.tui {
            let origin = object_origin.as_ref().context("release_gate")?;
            let object = tui_object_key(&release.url, &release.version, origin)?;
            store.authorize(id, task.revision, Usage::default()).await.context("authority")?;
            // Fetch into supervisor-owned staging. A failed fetch cannot fall back
            // to previously staged bytes.
            match tokio::fs::remove_file(&profile.artifact).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).context("artifact_fetch"),
            }
            local_command(&tui.fetch_command, &[object.as_ref(), profile.artifact.as_os_str()], "artifact_fetch").await?;
        }
        let digest = sha256_file(&profile.artifact)
            .await
            .context("artifact_digest")?;
        ensure!(digest == release.sha256, "artifact_digest");
        store
            .admit_effect(
                id,
                task.revision,
                Usage {
                    calls: 1,
                    ..Default::default()
                },
                &EffectIntent {
                    id: intent_id,
                    project_id: Some(task.project_id),
                    kind: EffectKind::FileReplace,
                    host_id: cfg.instance_id.to_string(),
                    resource: if profile.tui.is_some() {
                        format!("tui-upgrade:{}", release.version)
                    } else {
                        format!("upgrade:{}", release.version)
                    },
                    input_fingerprint: fingerprint(&requested)?,
                    actor_id: task.contract.specialist_id.clone(),
                    source: "agency-upgrade".into(),
                },
            )
            .await
            .context("authority")?;
        local_command(&profile.install_command, &[profile.artifact.as_os_str()], "installation").await?;
        ensure!(
            sha256_file(&profile.installed_artifact)
                .await
                .context("installed_digest")?
                == release.sha256,
            "installed_digest"
        );

        if let Some(tui) = &profile.tui {
            local_command(&tui.verify_command, &[profile.installed_artifact.as_os_str(), release.version.as_ref()], "tui_footer").await?;
            return Ok(json!({
                "installed_version": release.version,
                "installed_sha256": release.sha256,
                "companion_footer_verified": true,
            }));
        }
        let (companion_health, openbao_health) = health.context("startup")?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .context("startup")?;
        validate_endpoint(companion_health, true).context("startup")?;
        let mut healthy = false;
        for _ in 0..30 {
            if let Ok(response) = client.get(companion_health).send().await
                && let Ok(value) = bounded(response).await
                && value["version"] == release.version
                && value["status"] == "ok"
            {
                healthy = true;
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        ensure!(healthy, "startup_version");
        validate_endpoint(openbao_health, true).context("openbao_health")?;
        let response = client
            .get(openbao_health)
            .send()
            .await
            .context("openbao_health")?;
        ensure!(
            response.status() == reqwest::StatusCode::OK,
            "openbao_health"
        );
        let bao = bounded(response).await.context("openbao_health")?;
        ensure!(
            bao["initialized"] == true && bao["sealed"] == false,
            "openbao_health"
        );
        Ok::<_, anyhow::Error>(json!({
            "installed_version": release.version, "release": release, "startup": true, "openbao_ready": true,
        }))
    }
    .await;
    let (state, report, effect_status) = match result {
        Ok(report) => ("installed", report, EffectStatus::Confirmed),
        Err(e) => {
            let label = e.to_string();
            let safe = [
                "release_gate",
                "artifact_fetch",
                "artifact_digest",
                "installed_digest",
                "installer",
                "authority",
                "installation",
                "startup",
                "startup_version",
                "tui_footer",
                "peer_message",
                "openbao_health",
            ]
            .into_iter()
            .find(|s| *s == label)
            .unwrap_or("verification");
            (
                "failed",
                json!({"step":safe,"assistance_requested":true}),
                EffectStatus::Failed,
            )
        }
    };
    if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM agency_attempts WHERE id=$1)")
        .bind(intent_id)
        .fetch_one(&store.pool)
        .await?
    {
        effects::conclude(
            &store.pool,
            intent_id,
            Uuid::new_v4(),
            effect_status,
            EffectReason::Observed,
        )
        .await?;
    }
    let mut tx = store.pool.begin().await?;
    sqlx::query("UPDATE agency_upgrades SET state=$2, report=$3 WHERE task_id=$1")
        .bind(id)
        .bind(state)
        .bind(report)
        .execute(&mut *tx)
        .await?;
    let revision: Option<i64> = sqlx::query_scalar("UPDATE agency_tasks SET revision=revision+1 WHERE id=$1 AND contract->>'status'='working' RETURNING revision").bind(id).fetch_optional(&mut *tx).await?;
    if let Some(revision) = revision {
        enqueue(&mut tx, task.project_id, id, revision).await?;
    }
    tx.commit().await?;
    Ok(true)
}

async fn running_digest() -> Result<String> {
    #[cfg(target_os = "linux")]
    let executable = PathBuf::from("/proc/self/exe");
    #[cfg(not(target_os = "linux"))]
    let executable = std::env::current_exe()?;
    sha256_file(&executable).await
}

async fn sha256_file(path: &Path) -> Result<String> {
    use tokio::io::AsyncReadExt;
    let mut f = tokio::fs::File::open(path).await?;
    let mut hash = Sha256::new();
    let mut chunk = vec![0; 65536];
    loop {
        let count = f.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        hash.update(&chunk[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

async fn verify_roundtrip(
    store: &Store,
    cfg: &Config,
    task: &StoredTask,
    version: &str,
) -> Result<()> {
    let peer = cfg
        .peers
        .iter()
        .find(|peer| peer.id == task.requester)
        .context("probe peer missing")?;
    require_private_peer(cfg, &peer.id)?;
    let transport = Transport::new(store, cfg, peer)?;
    let check = json!({"caTask":task.contract.id,"publisherTask":Uuid::parse_str(&task.request.request_id)?,"nonce":Uuid::new_v4(),"version":version});
    let response = bounded(transport.call(Call::UpgradeCheck(&check)).await?).await?;
    ensure!(response == check, "peer did not acknowledge this rollout");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{trusted_object_origin, tui_object_key};

    #[test]
    fn tui_release_uses_only_the_ca_object_prefix_and_declared_version() {
        let valid = "https://objects.example.test/codifix-releases/ca/3.1.0-rc.3/tui-companion-footer-caf1d993";
        let origin =
            trusted_object_origin("https://objects.example.test/").expect("valid fixture origin");
        assert_eq!(
            tui_object_key(valid, "3.1.0-rc.3", &origin).ok().as_deref(),
            Some("codifix-releases/ca/3.1.0-rc.3/tui-companion-footer-caf1d993")
        );
        assert!(tui_object_key(valid, "3.1.0-rc.2", &origin).is_err());
        for invalid in [
            valid.replace("https://", "http://"),
            valid.replace("objects.example.test", "objects.example.test.example"),
            valid.replace("objects.example.test", "user@objects.example.test"),
            valid.replace("objects.example.test", "objects.example.test:443"),
            valid.replace("objects.example.test", "objects.example.test:8443"),
            valid.replace("/ca/", "/ca/other/../"),
            valid.replace("tui-companion-footer-", "worker-"),
            format!("{valid}?X-Amz-Signature=secret"),
        ] {
            assert!(
                tui_object_key(&invalid, "3.1.0-rc.3", &origin).is_err(),
                "{invalid}"
            );
        }
        let custom = trusted_object_origin("https://objects.example.test:8443/")
            .expect("valid fixture origin");
        let custom_url = valid.replace("objects.example.test", "objects.example.test:8443");
        assert!(tui_object_key(&custom_url, "3.1.0-rc.3", &custom).is_ok());
        for invalid in [
            "",
            "http://objects.example.test/",
            "https://objects.example.test/path",
            "https://objects.example.test/?key=value",
        ] {
            assert!(trusted_object_origin(invalid).is_err());
        }
    }
}
