// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bind prepared/query vectors to exact inputs and a locked embedding profile.
//! These bindings detect mismatches; they do not attest that a provider actually
//! produced a vector. Provider receipts and campaign approval remain independent.
use super::baselines::Document;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub provider: String,
    pub model: String,
    pub dimensions: usize,
    pub normalization: String,
    pub document_purpose: String,
    pub query_purpose: String,
    pub metric: String,
}

impl Profile {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=8192).contains(&self.dimensions)
                && self.metric == "cosine"
                && [
                    &self.provider,
                    &self.model,
                    &self.normalization,
                    &self.document_purpose,
                    &self.query_purpose,
                ]
                .iter()
                .all(|value| !value.is_empty()
                    && value.len() <= 256
                    && value.trim() == value.as_str()
                    && value.bytes().all(|byte| byte.is_ascii_graphic())),
            "invalid embedding profile"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentBinding {
    pub document_id: String,
    pub text_sha256: String,
    pub vector_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    pub profile: Profile,
    pub bindings: Vec<DocumentBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub profile: Profile,
    pub text_sha256: String,
    pub vector_sha256: String,
}

pub fn text_digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// IEEE-754 binary64 little-endian bytes avoid JSON float-formatting differences
/// between Rust and the trusted-host mediator. Vectors must already be finite.
pub fn vector_digest(vector: &[f64]) -> String {
    let mut digest = Sha256::new();
    for value in vector {
        digest.update(value.to_le_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn valid_vector(profile: &Profile, vector: &[f64]) -> bool {
    vector.len() == profile.dimensions
        && vector.iter().all(|value| value.is_finite())
        && vector.iter().any(|value| *value != 0.0)
}

pub fn validate(
    corpus: &Corpus,
    documents: &[Document],
    vectors: &[Vec<f64>],
    query: &Query,
    text: &str,
    vector: &[f64],
) -> Result<()> {
    corpus.profile.validate()?;
    query.profile.validate()?;
    ensure!(
        corpus.profile == query.profile,
        "embedding profile mismatch"
    );
    ensure!(
        !documents.is_empty()
            && documents.len() == vectors.len()
            && documents.len() == corpus.bindings.len(),
        "embedding corpus coverage mismatch"
    );
    for ((document, values), binding) in documents.iter().zip(vectors).zip(&corpus.bindings) {
        ensure!(
            binding.document_id == document.id
                && binding.text_sha256 == text_digest(&document.text)
                && valid_vector(&corpus.profile, values)
                && binding.vector_sha256 == vector_digest(values),
            "embedding document binding mismatch"
        );
    }
    ensure!(
        query.text_sha256 == text_digest(text)
            && valid_vector(&query.profile, vector)
            && query.vector_sha256 == vector_digest(vector),
        "embedding query binding mismatch"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_preserves_float32_vectors_promoted_to_binary64() {
        // Hashes bind exact binary64 values, including promoted float32 storage
        // values whose shortest JSON decimal is not exactly representable.
        for bits in 0x3b00_0001..0x3b00_1001 {
            let value = f64::from(f32::from_bits(bits));
            let json = serde_json::to_string(&value).expect("finite vector value");
            let decoded: f64 = serde_json::from_str(&json).expect("vector JSON");
            assert_eq!(decoded.to_bits(), value.to_bits(), "changed {json}");
            assert_eq!(vector_digest(&[decoded]), vector_digest(&[value]));
        }
    }

    pub(super) fn profile() -> Profile {
        Profile {
            provider: "synthetic".into(),
            model: "unit-test-r1".into(),
            dimensions: 2,
            normalization: "identity-r1".into(),
            document_purpose: "document".into(),
            query_purpose: "query".into(),
            metric: "cosine".into(),
        }
    }

    #[test]
    fn exact_vectors_text_and_profiles_are_bound_without_provider_claims() {
        let documents = vec![
            Document {
                id: "a".into(),
                text: "source one".into(),
            },
            Document {
                id: "b".into(),
                text: "source two".into(),
            },
        ];
        let vectors = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let corpus = Corpus {
            profile: profile(),
            bindings: documents
                .iter()
                .zip(&vectors)
                .map(|(doc, values)| DocumentBinding {
                    document_id: doc.id.clone(),
                    text_sha256: text_digest(&doc.text),
                    vector_sha256: vector_digest(values),
                })
                .collect(),
        };
        let query = Query {
            profile: profile(),
            text_sha256: text_digest("question"),
            vector_sha256: vector_digest(&[1.0, 0.0]),
        };
        assert!(
            validate(
                &corpus,
                &documents,
                &vectors,
                &query,
                "question",
                &[1.0, 0.0]
            )
            .is_ok()
        );
        for changed in [
            Profile {
                model: "unit-test-r2".into(),
                ..profile()
            },
            Profile {
                normalization: "changed".into(),
                ..profile()
            },
            Profile {
                query_purpose: "document".into(),
                ..profile()
            },
        ] {
            let bad = Query {
                profile: changed,
                ..query.clone()
            };
            assert!(
                validate(&corpus, &documents, &vectors, &bad, "question", &[1.0, 0.0]).is_err()
            );
        }
        assert!(
            validate(
                &corpus,
                &documents,
                &vectors,
                &query,
                "changed question",
                &[1.0, 0.0]
            )
            .is_err()
        );
        assert!(
            validate(
                &corpus,
                &documents,
                &vectors,
                &query,
                "question",
                &[0.0, 1.0]
            )
            .is_err()
        );
        let mut changed_documents = documents.clone();
        changed_documents[0].text.push_str(" changed");
        assert!(
            validate(
                &corpus,
                &changed_documents,
                &vectors,
                &query,
                "question",
                &[1.0, 0.0]
            )
            .is_err()
        );
        let mut reversed = vectors.clone();
        reversed.reverse();
        assert!(
            validate(
                &corpus,
                &documents,
                &reversed,
                &query,
                "question",
                &[1.0, 0.0]
            )
            .is_err()
        );
        assert!(
            validate(
                &corpus,
                &documents[..1],
                &vectors[..1],
                &query,
                "question",
                &[1.0, 0.0]
            )
            .is_err()
        );
    }

    #[test]
    fn dimensions_invalid_values_and_unknown_contract_fields_fail_closed() {
        for dimensions in [0, 8193] {
            assert!(
                Profile {
                    dimensions,
                    ..profile()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            Profile {
                metric: "dot".into(),
                ..profile()
            }
            .validate()
            .is_err()
        );
        assert!(!valid_vector(&profile(), &[0.0, 0.0]));
        assert!(!valid_vector(&profile(), &[f64::NAN, 1.0]));
        assert!(!valid_vector(&profile(), &[f64::INFINITY, 1.0]));
        assert!(!valid_vector(&profile(), &[1.0]));
        let mut value = serde_json::to_value(profile()).expect("profile");
        value["silently_use_latest"] = true.into();
        assert!(serde_json::from_value::<Profile>(value).is_err());
    }
}
