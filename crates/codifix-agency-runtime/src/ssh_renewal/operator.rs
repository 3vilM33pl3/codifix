//! Local operator console. Step prompts for its normal provisioner credentials.
use super::*;
use crate::{config::Config, discovery::bounded, store::StoredTask};
use std::path::Path;

pub async fn decide(
    cfg: &Config,
    actor: &str,
    task_id: Uuid,
    reject: Option<&str>,
    step_password_file: Option<&Path>,
) -> Result<Value> {
    let peer = cfg
        .peers
        .iter()
        .find(|p| p.id == actor && p.operator && p.instance_id == cfg.instance_id)
        .context("named local operator required")?;
    let credential = std::env::var(&peer.token_env)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let url = format!("http://{}", cfg.listen);
    let stored: StoredTask = serde_json::from_value(
        bounded(
            client
                .get(format!("{url}/v1/tasks/{task_id}"))
                .bearer_auth(&credential)
                .send()
                .await?,
        )
        .await?,
    )?;
    ensure!(
        stored.request.capability == "ssh_host_sign"
            && stored.request.resource.kind == "ssh_host"
            && stored.contract.status == TaskStatus::InputRequired,
        "expected pending host certificate request"
    );
    let host = &stored.request.resource.id;
    ensure!(
        operator_allowed(host, actor)?,
        "operator not enrolled for this host"
    );
    let p = profile(host)?;
    let request = &stored.request.payload["request"];
    let check = adapter(
        json!({"host":host,"operation":"review","request":request,"requester":stored.requester}),
    )
    .await?;
    ensure!(check.get("ok").is_some(), "host or requester failed review");
    let answer = if let Some(reason) = reject {
        ensure!(
            !reason.trim().is_empty() && reason.len() <= 256,
            "a short rejection reason is required"
        );
        format!("reject: {reason}")
    } else {
        eprintln!(
            "Approving SSH host {} with existing key {} and principals {} for renewal {}.",
            host, request["fingerprint"], request["principals"], task_id
        );
        let stage = Path::new(p["stage_dir"].as_str().context("missing stage directory")?);
        let target = stage.join(format!("{task_id}-cert.pub"));
        if !target.try_exists()? {
            let dir = stage.join(format!(".renew-{}", Uuid::new_v4()));
            std::fs::create_dir(&dir)?;
            let result = async {
                let pubkey = dir.join("host.pub");
                std::fs::write(
                    &pubkey,
                    request["public_key"]
                        .as_str()
                        .context("missing host public key")?,
                )?;
                let mut step = Command::new("step");
                step.args(["ssh", "certificate", host]).arg(&pubkey).args([
                    "--sign",
                    "--host",
                    "--no-agent",
                    "--ca-url",
                    p["step_url"].as_str().context("Step CA URL required")?,
                    "--root",
                    p["step_root"].as_str().context("Step CA root required")?,
                    "--issuer",
                    p["step_issuer"]
                        .as_str()
                        .context("Step provisioner required")?,
                ]);
                for principal in request["principals"]
                    .as_array()
                    .context("missing principals")?
                {
                    step.arg("--principal")
                        .arg(principal.as_str().context("invalid principal")?);
                }
                if let Some(duration) = p["step_duration"].as_str() {
                    step.arg("--not-after").arg(duration);
                }
                if let Some(path) = step_password_file {
                    step.arg("--provisioner-password-file").arg(path);
                }
                ensure!(
                    step.status().await?.success(),
                    "Step signing did not complete"
                );
                std::fs::hard_link(dir.join("host-cert.pub"), &target)?;
                Ok::<(), anyhow::Error>(())
            }
            .await;
            std::fs::remove_dir_all(&dir)?;
            result?;
        }
        "approve".into()
    };
    let message = json!({"message":{"messageId":Uuid::new_v4(),"role":"ROLE_USER","taskId":task_id,"contextId":task_id,"parts":[{"data":{"answer":answer}}]},"configuration":{"returnImmediately":true}});
    bounded(
        client
            .post(format!("{url}/message:send"))
            .header("A2A-Version", "1.0")
            .bearer_auth(credential)
            .json(&message)
            .send()
            .await?,
    )
    .await
}
