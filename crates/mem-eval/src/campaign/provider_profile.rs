// SPDX-License-Identifier: AGPL-3.0-or-later
//! Frozen provider configuration, not approval or proof of tokenizer correctness.
//! Referenced files are verified by the normal manifest asset boundary.
use super::{AssetKind, Manifest, identifier, relative_path, valid_digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tokenizer {
    pub id: String,
    pub asset: Pin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Embedding {
    pub resolved_id: String,
    pub dimensions: u32,
    pub max_input_tokens: u64,
    pub input_microusd_per_million: u64,
    pub tokenizer: Tokenizer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderProfile {
    pub version: u8,
    /// Tool-free Responses and single-input float32 embeddings only.
    pub protocol: String,
    pub responses_tokenizer: Tokenizer,
    pub embedding: Embedding,
    /// Reviewed model/tokenizer/rate provenance, itself a locked private asset.
    /// A hash establishes identity; it does not make the contents trustworthy.
    pub configuration_evidence: Pin,
}

impl Pin {
    fn validate(&self, manifest: &Manifest) -> Result<()> {
        ensure!(
            relative_path(&self.path)
                && valid_digest(&self.sha256)
                && manifest.assets.iter().any(|asset| {
                    asset.path == self.path
                        && asset.sha256 == self.sha256
                        && asset.kind == AssetKind::Configuration
                }),
            "provider profile references an unlocked configuration asset"
        );
        Ok(())
    }
}

impl ProviderProfile {
    pub fn validate(&self, manifest: &Manifest) -> Result<()> {
        const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
        let model = &manifest.model;
        ensure!(
            self.version == 1
                && self.protocol == "openai-text-v1"
                && model.provider == "openai"
                && identifier(&self.embedding.resolved_id)
                && self.embedding.resolved_id == model.embedding_id
                && (1..=8192).contains(&self.embedding.dimensions)
                && (1..=8192).contains(&self.embedding.max_input_tokens)
                && identifier(&self.responses_tokenizer.id)
                && identifier(&self.embedding.tokenizer.id)
                && [
                    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"
                ]
                .contains(&model.reasoning.as_str()),
            "unsupported or mismatched provider profile"
        );
        ensure!(
            [
                model.max_input_tokens,
                model.max_output_tokens,
                model.input_microusd_per_million,
                model.output_microusd_per_million,
                model.cached_input_microusd_per_million.unwrap_or(0),
                self.embedding.input_microusd_per_million,
            ]
            .into_iter()
            .all(|value| value <= MAX_SAFE_INTEGER)
                && model
                    .max_input_tokens
                    .checked_add(model.max_output_tokens)
                    .is_some_and(|sum| sum <= MAX_SAFE_INTEGER),
            "provider profile exceeds portable integer bounds"
        );
        self.responses_tokenizer.asset.validate(manifest)?;
        self.embedding.tokenizer.asset.validate(manifest)?;
        self.configuration_evidence.validate(manifest)?;
        Ok(())
    }
}
