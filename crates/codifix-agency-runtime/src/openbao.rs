use crate::{
    config::Config,
    store::{Store, StoredTask},
};
use anyhow::Result;
use serde_json::Value;
pub async fn recover(_store: &Store, _cfg: &Config, _task: &StoredTask) -> Result<Value> {
    anyhow::bail!("OpenBao controller not linked")
}
