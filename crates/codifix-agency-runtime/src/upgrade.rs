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
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
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
    ensure!(
        task.request.resource.kind == "installation"
            && task.request.resource.id == cfg.instance_id.to_string(),
        "installation scope mismatch"
    );
    let release: Release = serde_json::from_value(task.request.payload["release"].clone())?;
    release.validate()?;
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
        let failed_step = if release.version != env!("CARGO_PKG_VERSION") {
            Some("running_version")
        } else if !running_digest()
            .await
            .is_ok_and(|digest| digest == release.sha256)
        {
            Some("running_digest")
        } else if verify_roundtrip(store, cfg, task).await.is_err() {
            Some("post_restart_peer_message")
        } else {
            None
        };
        let (state, result) = if let Some(step) = failed_step {
            ("failed", json!({"step":step,"assistance_requested":true}))
        } else {
            report["peer_message"] = json!(true);
            report["running_sha256"] = json!(release.sha256);
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
struct Profile {
    publisher_peer: String,
    release_receipt: PathBuf,
    artifact: PathBuf,
    installed_artifact: PathBuf,
    install_command: Vec<String>,
    companion_health: String,
    openbao_health: String,
}

pub async fn supervise(store: &Store, cfg: &Config, path: &Path) -> Result<bool> {
    let profile: Profile = serde_json::from_slice(&std::fs::read(path)?)?;
    // The profile, local receipt and staged artifact are administrator-controlled.
    // No destination, executable or package path comes from a remote message.
    let release: Release = serde_json::from_slice(&std::fs::read(&profile.release_receipt)?)?;
    release.validate()?;
    let mut tx = store.pool.begin().await?;
    // Serialize intake on this CA even if two timer invocations overlap.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("upgrade:{}", cfg.instance_id))
        .execute(&mut *tx)
        .await?;
    let next: Option<(Uuid, Value)> = sqlx::query_as("SELECT task_id,release FROM agency_upgrades WHERE instance_id=$1 AND state='queued' AND NOT EXISTS (SELECT 1 FROM agency_upgrades busy WHERE busy.instance_id=$1 AND busy.state IN ('installing','installed')) ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED")
        .bind(cfg.instance_id).fetch_optional(&mut *tx).await?;
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
        ensure!(
            task.requester == profile.publisher_peer
                && serde_json::to_value(&release)? == requested,
            "release_gate"
        );
        let digest = sha256_file(&profile.artifact)
            .await
            .context("artifact_digest")?;
        ensure!(digest == release.sha256, "artifact_digest");
        let (program, args) = profile.install_command.split_first().context("installer")?;
        ensure!(Path::new(program).is_absolute(), "installer");
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
                    resource: format!("upgrade:{}", release.version),
                    input_fingerprint: fingerprint(&requested)?,
                    actor_id: task.contract.specialist_id.clone(),
                    source: "agency-upgrade".into(),
                },
            )
            .await
            .context("authority")?;
        let mut install = Command::new(program);
        install.args(args).arg(&profile.artifact).kill_on_drop(true);
        install
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let status = tokio::time::timeout(Duration::from_secs(300), install.status())
            .await
            .context("installation")?
            .context("installation")?;
        ensure!(status.success(), "installation");
        ensure!(
            sha256_file(&profile.installed_artifact)
                .await
                .context("installed_digest")?
                == release.sha256,
            "installed_digest"
        );
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .context("startup")?;
        validate_endpoint(&profile.companion_health, true).context("startup")?;
        let mut healthy = false;
        for _ in 0..30 {
            if let Ok(response) = client.get(&profile.companion_health).send().await
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
        validate_endpoint(&profile.openbao_health, true).context("openbao_health")?;
        let response = client
            .get(&profile.openbao_health)
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
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let (state, report, effect_status) = match result {
        Ok(()) => (
            "installed",
            json!({"installed_version":release.version,"release":release,"startup":true,"openbao_ready":true}),
            EffectStatus::Confirmed,
        ),
        Err(e) => {
            let label = e.to_string();
            let safe = [
                "release_gate",
                "artifact_digest",
                "installed_digest",
                "installer",
                "authority",
                "installation",
                "startup",
                "startup_version",
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

async fn verify_roundtrip(store: &Store, cfg: &Config, task: &StoredTask) -> Result<()> {
    let peer = cfg
        .peers
        .iter()
        .find(|peer| peer.id == task.requester)
        .context("probe peer missing")?;
    ensure!(
        cfg.social
            .as_ref()
            .is_some_and(|s| s.peers.contains_key(&peer.id)),
        "private XRPC required"
    );
    let transport = Transport::new(store, cfg, peer)?;
    let check = json!({"caTask":task.contract.id,"publisherTask":Uuid::parse_str(&task.request.request_id)?,"nonce":Uuid::new_v4(),"version":env!("CARGO_PKG_VERSION")});
    let response = bounded(transport.call(Call::UpgradeCheck(&check)).await?).await?;
    ensure!(response == check, "peer did not acknowledge this rollout");
    Ok(())
}
