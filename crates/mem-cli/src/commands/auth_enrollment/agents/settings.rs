// SPDX-License-Identifier: AGPL-3.0-or-later
//! Secret-free project MCP entry; custom settings bytes never enter metadata.
use super::super::{
    CompanyOrigin,
    credentials::{self, CredentialStore},
};
#[cfg(not(unix))]
use anyhow::bail;
use anyhow::{Result, ensure};
use mem_record::CompanyAgentCaller;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[cfg(unix)]
mod filesystem;
#[cfg(test)]
mod tests;
const MAX_BYTES: usize = 256 * 1024;
fn bytes_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn digest(value: &impl Serialize) -> Result<String> {
    Ok(bytes_hash(&serde_json::to_vec(value)?))
}
fn changed() -> anyhow::Error {
    anyhow::anyhow!(
        "settings or reviewed configuration inputs changed; preview again before confirming"
    )
}
fn invalid() -> anyhow::Error {
    anyhow::anyhow!("project settings are invalid or incompatible; existing bytes preserved")
}
fn text(path: &Path) -> Result<String> {
    let value = path.to_str().ok_or_else(invalid)?;
    ensure!(
        path.is_absolute() && value.len() <= 4096 && !value.chars().any(char::is_control),
        "bounded absolute settings/profile/executable path required"
    );
    Ok(value.into())
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Inputs {
    pub company: String,
    pub agent_id: Uuid,
    pub caller: CompanyAgentCaller,
    pub provider_id: String,
    pub device_id: Uuid,
    pub scope: Value,
    pub workspace: PathBuf,
    pub profile: PathBuf,
    pub workspace_journal_sha256: String,
    pub profile_sha256: String,
    pub bootstrap_sha256: String,
    pub manifest_sha256: String,
    pub receipt_sha256: String,
    pub command: String,
    pub allow_loopback: bool,
    pub allow_offline: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    inputs: Inputs,
    settings_path: PathBuf,
    key: String,
    args: Vec<String>,
    before_sha256: Option<String>,
    after_sha256: String,
    stage_id: Uuid,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    plan: Plan,
    plan_sha256: String,
    completed: bool,
}
fn owned_key(inputs: &Inputs) -> Result<String> {
    Ok(format!(
        "memory_project_{}",
        &digest(&(inputs.agent_id, &inputs.scope))?[..24]
    ))
}
fn arguments(inputs: &Inputs) -> Result<Vec<String>> {
    ensure!(
        inputs.provider_id == inputs.caller.provider_id()
            && !inputs.agent_id.is_nil()
            && !inputs.device_id.is_nil(),
        "exact independent agent inputs required"
    );
    let mut args = vec![
        "projects".into(),
        "agents".into(),
        "mcp".into(),
        "--company".into(),
        inputs.company.clone(),
        "--agent-id".into(),
        inputs.agent_id.to_string(),
        "--profile".into(),
        text(&inputs.profile)?,
    ];
    CompanyOrigin::parse(&inputs.company, inputs.allow_loopback)?;
    if inputs.allow_loopback {
        args.push("--allow-loopback".into());
    }
    if inputs.allow_offline {
        args.push("--allow-offline".into());
    }
    text(Path::new(&inputs.command))?;
    Ok(args)
}
fn relative(caller: CompanyAgentCaller) -> &'static str {
    match caller {
        CompanyAgentCaller::Codex => ".codex/config.toml",
        CompanyAgentCaller::Claude => ".mcp.json",
    }
}
fn entry(caller: CompanyAgentCaller, command: &str, args: &[String]) -> Value {
    match caller {
        CompanyAgentCaller::Codex => json!({"command":command,"args":args}),
        CompanyAgentCaller::Claude => json!({"type":"stdio","command":command,"args":args}),
    }
}
// serde_json::Value alone accepts duplicate object keys and drops earlier
// values. Reject ambiguity rather than claiming that custom values survived.
struct UniqueJson(Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                v: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::Bool(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::Number(
                    serde_json::Number::from_f64(v).ok_or_else(|| E::custom("invalid number"))?,
                )))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::String(v.into())))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                v: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::String(v)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueJson(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(UniqueJson(Value::Array(values)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some((key, UniqueJson(value))) = map.next_entry::<String, UniqueJson>()? {
                    if values.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate key"));
                    }
                }
                Ok(UniqueJson(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
fn merge(
    caller: CompanyAgentCaller,
    original: Option<&[u8]>,
    key: &str,
    command: &str,
    args: &[String],
) -> Result<Vec<u8>> {
    let original = original.unwrap_or_default();
    ensure!(
        original.len() <= MAX_BYTES,
        "settings exceed bounded size; existing bytes preserved"
    );
    let expected = entry(caller, command, args);
    match caller {
        CompanyAgentCaller::Codex => {
            let source = std::str::from_utf8(original).map_err(|_| invalid())?;
            let parsed: toml::Value = toml::from_str(source).map_err(|_| invalid())?;
            if let Some(servers) = parsed.get("mcp_servers") {
                let servers = servers.as_table().ok_or_else(invalid)?;
                if let Some(existing) = servers.get(key) {
                    if serde_json::to_value(existing).map_err(|_| invalid())? != expected {
                        return Err(changed());
                    }
                    return Ok(original.to_vec());
                }
            }
            #[derive(Serialize)]
            struct Entry<'a> {
                command: &'a str,
                args: &'a [String],
            }
            let mut result = original.to_vec();
            if !result.is_empty() && !result.ends_with(b"\n") {
                result.push(b'\n');
            }
            result.extend_from_slice(format!("\n[mcp_servers.{key}]\n").as_bytes());
            result.extend_from_slice(
                toml::to_string(&Entry { command, args })
                    .map_err(|_| invalid())?
                    .as_bytes(),
            );
            let _: toml::Value =
                toml::from_str(std::str::from_utf8(&result).map_err(|_| invalid())?)
                    .map_err(|_| invalid())?;
            ensure!(result.len() <= MAX_BYTES, "merged settings exceed bound");
            Ok(result)
        }
        CompanyAgentCaller::Claude => {
            let mut parsed: Value = if original.is_empty() {
                json!({})
            } else {
                serde_json::from_slice::<UniqueJson>(original)
                    .map_err(|_| invalid())?
                    .0
            };
            let root = parsed.as_object_mut().ok_or_else(invalid)?;
            let servers = root
                .entry("mcpServers")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(invalid)?;
            if let Some(existing) = servers.get(key) {
                if existing != &expected {
                    return Err(changed());
                }
                return Ok(original.to_vec());
            }
            servers.insert(key.into(), expected);
            let mut result = serde_json::to_vec_pretty(&parsed).map_err(|_| invalid())?;
            result.push(b'\n');
            ensure!(result.len() <= MAX_BYTES, "merged settings exceed bound");
            Ok(result)
        }
    }
}
fn account(inputs: &Inputs) -> Result<String> {
    Ok(format!(
        "settings:{}",
        digest(
            &json!({"agent":inputs.agent_id,"workspace":inputs.workspace,"caller":inputs.caller})
        )?
    ))
}
fn service(origin: &CompanyOrigin) -> String {
    format!(
        "net.memory-layer.agent-settings:{}",
        credentials::sha256(&origin.0)
    )
}
fn read_journal(
    store: &impl CredentialStore,
    origin: &CompanyOrigin,
    inputs: &Inputs,
) -> Result<Option<Journal>> {
    let Some(bytes) = store.get(&service(origin), &account(inputs)?)? else {
        return Ok(None);
    };
    ensure!(
        bytes.len() <= 65536,
        "settings Keychain metadata exceeds bound"
    );
    let journal: Journal = serde_json::from_slice(&bytes).map_err(|_| changed())?;
    ensure!(
        journal.schema_version == 1
            && journal.plan_sha256 == digest(&journal.plan)?
            && !journal.plan.stage_id.is_nil(),
        "settings Keychain plan changed"
    );
    Ok(Some(journal))
}
fn save(store: &impl CredentialStore, origin: &CompanyOrigin, journal: &Journal) -> Result<()> {
    store.put(
        &service(origin),
        &account(&journal.plan.inputs)?,
        &serde_json::to_vec(journal)?,
    )
}

#[cfg(unix)]
pub(super) struct Prepared {
    io: filesystem::Settings,
    journal: Journal,
    result: Vec<u8>,
    already: bool,
}
#[cfg(not(unix))]
pub(super) struct Prepared;
#[cfg(unix)]
pub(super) fn prepare(
    store: &impl CredentialStore,
    origin: &CompanyOrigin,
    inputs: Inputs,
    journal_path: &Path,
    expected: Option<&str>,
) -> Result<Prepared> {
    ensure!(inputs.company == origin.0, "settings company changed");
    let settings_path = inputs.workspace.join(relative(inputs.caller));
    let io = filesystem::Settings::open(&inputs.workspace, inputs.caller, journal_path)?;
    let original = io.read()?;
    let before = original.as_deref().map(bytes_hash);
    let args = arguments(&inputs)?;
    let key = owned_key(&inputs)?;
    let result = merge(
        inputs.caller,
        original.as_deref(),
        &key,
        &inputs.command,
        &args,
    )?;
    let after = bytes_hash(&result);
    let existing = read_journal(store, origin, &inputs)?;
    let journal = if let Some(journal) = existing.filter(|j| {
        j.plan.inputs == inputs
            && (j.plan.before_sha256 == before || before.as_ref() == Some(&j.plan.after_sha256))
    }) {
        journal
    } else {
        ensure!(
            expected.is_none(),
            "saved settings preview changed or missing; no local write performed"
        );
        let plan = Plan {
            inputs,
            settings_path,
            key,
            args,
            before_sha256: before.clone(),
            after_sha256: after,
            stage_id: Uuid::new_v4(),
        };
        Journal {
            schema_version: 1,
            plan_sha256: digest(&plan)?,
            plan,
            completed: false,
        }
    };
    if let Some(expected) = expected {
        ensure!(
            expected == journal.plan_sha256,
            "confirmation digest does not match saved settings preview"
        );
    }
    ensure!(
        journal.plan.key == owned_key(&journal.plan.inputs)?
            && journal.plan.args == arguments(&journal.plan.inputs)?
            && journal.plan.settings_path
                == journal
                    .plan
                    .inputs
                    .workspace
                    .join(relative(journal.plan.inputs.caller)),
        "saved settings metadata changed"
    );
    let already = before.as_ref() == Some(&journal.plan.after_sha256);
    ensure!(
        already
            || (before == journal.plan.before_sha256
                && bytes_hash(&result) == journal.plan.after_sha256),
        "settings bytes changed; no local writes performed"
    );
    save(store, origin, &journal)?;
    Ok(Prepared {
        io,
        journal,
        result,
        already,
    })
}
#[cfg(not(unix))]
pub(super) fn prepare(
    _: &impl CredentialStore,
    _: &CompanyOrigin,
    _: Inputs,
    _: &Path,
    _: Option<&str>,
) -> Result<Prepared> {
    bail!("protected project settings writes unsupported on this platform")
}
#[cfg(unix)]
impl Prepared {
    pub(super) fn inputs(&self) -> &Inputs {
        &self.journal.plan.inputs
    }
    pub(super) fn preview(&self, committed: bool, replayed: bool) -> Value {
        let p = &self.journal.plan;
        let i = &p.inputs;
        json!({"schema_version":1,"plan_sha256":self.journal.plan_sha256,"agent_id":i.agent_id,"caller":i.caller,"provider_id":i.provider_id,"device_id":i.device_id,"scope":i.scope,"workspace":i.workspace,"settings_path":p.settings_path,"before_sha256":p.before_sha256,"after_sha256":p.after_sha256,"command":i.command,"args":p.args,"allow_offline":i.allow_offline,"preserves_other_settings":true,"contains_credentials":false,"already_configured":self.already,"committed":committed,"replayed":replayed})
    }
    pub(super) fn apply(
        mut self,
        store: &impl CredentialStore,
        origin: &CompanyOrigin,
    ) -> Result<Value> {
        self.io.replace(
            self.journal.plan.before_sha256.as_deref(),
            &self.journal.plan.after_sha256,
            &self.result,
            self.journal.plan.stage_id,
        )?;
        self.journal.completed = true;
        save(store, origin, &self.journal)?;
        let replayed = self.already;
        self.already = true;
        Ok(self.preview(true, replayed))
    }
}
#[cfg(not(unix))]
impl Prepared {
    pub(super) fn inputs(&self) -> &Inputs {
        unreachable!("unsupported platform prepare always fails")
    }
    pub(super) fn preview(&self, _: bool, _: bool) -> Value {
        Value::Null
    }
    pub(super) fn apply(self, _: &impl CredentialStore, _: &CompanyOrigin) -> Result<Value> {
        bail!("protected settings writes unsupported on this platform")
    }
}
