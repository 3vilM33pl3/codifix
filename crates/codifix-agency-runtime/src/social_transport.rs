//! Route a private exchange through the enrolled peer's configured transport.
use crate::{
    config::{Config, Peer},
    discovery::bounded,
    social::SocialConfig,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use reqwest::{Client, Response};
use serde_json::{Value, json};

pub(crate) enum Call<'a> {
    UpgradeCheck(&'a Value),
    Send(&'a Value),
    IncidentNotice(&'a Value),
    Notice(&'a Value),
    Status(&'a str),
    Renew(&'a str),
    Cancel(&'a str),
    Lookup(&'a str),
}

pub(crate) struct Transport<'a> {
    store: &'a Store,
    cfg: &'a Config,
    peer: &'a Peer,
    endpoint: &'a str,
    client: Client,
}

impl<'a> Transport<'a> {
    pub(crate) fn new(store: &'a Store, cfg: &'a Config, peer: &'a Peer) -> Result<Self> {
        let endpoint = peer
            .endpoint
            .as_deref()
            .context("peer has no endpoint")?
            .trim_end_matches('/');
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self {
            store,
            cfg,
            peer,
            endpoint,
            client,
        })
    }

    pub(crate) async fn call(&self, call: Call<'_>) -> Result<Response> {
        if let Some(social) = &self.cfg.social
            && let Some(binding) = social.peers.get(&self.peer.id)
        {
            let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agency_social_moderation WHERE instance_id=$1 AND peer_id=$2 AND (banned_until IS NULL OR banned_until>clock_timestamp()))")
                .bind(self.cfg.instance_id).bind(&self.peer.id).fetch_one(&self.store.pool).await?;
            ensure!(!blocked, "peer blocked by local moderation");
            return self.xrpc(social, &binding.did, call).await;
        }
        let token = std::env::var(
            self.peer
                .outbound_token_env
                .as_ref()
                .context("peer outbound credential absent")?,
        )
        .context("peer credential unavailable")?;
        let endpoint = self.endpoint;
        let request = match call {
            Call::IncidentNotice(_) | Call::UpgradeCheck(_) | Call::Notice(_) => {
                anyhow::bail!("notices require private AT transport")
            }
            Call::Send(body) => self
                .client
                .post(format!("{endpoint}/message:send"))
                .json(body),
            Call::Status(id) => self
                .client
                .get(format!("{endpoint}/tasks/{}", urlencoding::encode(id))),
            Call::Renew(id) => self
                .client
                .post(format!(
                    "{endpoint}/v1/tasks/{}/lease",
                    urlencoding::encode(id)
                ))
                .json(&json!({})),
            Call::Cancel(id) => self
                .client
                .post(format!(
                    "{endpoint}/tasks/{}:cancel",
                    urlencoding::encode(id)
                ))
                .json(&json!({})),
            Call::Lookup(id) => self.client.get(format!(
                "{endpoint}/v1/task-requests/{}",
                urlencoding::encode(id)
            )),
        };
        Ok(request
            .header("A2A-Version", "1.0")
            .header("Content-Type", "application/a2a+json")
            .bearer_auth(token)
            .send()
            .await?)
    }

    async fn xrpc(&self, social: &SocialConfig, did: &str, call: Call<'_>) -> Result<Response> {
        let operation = match call {
            Call::Notice(_) => "deliverNotice",
            Call::IncidentNotice(_) => "deliverIncidentNotice",
            Call::UpgradeCheck(_) => "checkUpgrade",
            Call::Send(_) => "sendMessage",
            Call::Status(_) => "getTask",
            Call::Renew(_) => "renewLease",
            Call::Cancel(_) => "cancelTask",
            Call::Lookup(_) => "findRequest",
        };
        let audience = crate::social::service_did(did);
        let method = format!("dev.codifix.agency.{operation}");
        let access = std::env::var(&social.access_token_env).context("PDS session unavailable")?;
        let response = self
            .client
            .get(format!(
                "{}/xrpc/com.atproto.server.getServiceAuth",
                social.pds.trim_end_matches('/')
            ))
            .bearer_auth(access)
            .query(&[("aud", audience.as_str()), ("lxm", method.as_str())])
            .send()
            .await?;
        let auth = bounded(response).await?;
        let token = auth["token"]
            .as_str()
            .context("missing PDS service token")?;
        social.check_outbound_identity(token, &audience, &method)?;
        let url = format!("{}/xrpc/{method}", self.endpoint);
        let request = match call {
            Call::IncidentNotice(body)
            | Call::Send(body)
            | Call::Notice(body)
            | Call::UpgradeCheck(body) => self.client.post(url).json(body),
            Call::Status(id) | Call::Lookup(id) => self.client.get(url).query(&[("id", id)]),
            Call::Renew(id) | Call::Cancel(id) => self.client.post(url).json(&json!({"id":id})),
        };
        Ok(request.bearer_auth(token).send().await?)
    }
}
