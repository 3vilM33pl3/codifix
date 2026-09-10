// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fixed raw-corpus comparators: no Memory identifiers, graph or learned scores.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub id: String,
    pub text: String,
    pub score: f64,
}

fn terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Stable paragraph chunks, bounded in Unicode characters; no question-dependent
/// preprocessing. Chunk IDs retain source attribution across both comparators.
pub fn chunks(documents: &[Document], characters: usize) -> Result<Vec<Document>> {
    ensure!((128..=8192).contains(&characters), "invalid chunk size");
    let mut output = Vec::new();
    let mut ids = BTreeSet::new();
    for doc in documents {
        ensure!(
            !doc.id.is_empty() && ids.insert(&doc.id),
            "empty/duplicate source id"
        );
        for (index, slice) in doc
            .text
            .chars()
            .collect::<Vec<_>>()
            .chunks(characters)
            .enumerate()
        {
            output.push(Document {
                id: format!("{}#{index}", doc.id),
                text: slice.iter().collect(),
            });
        }
    }
    Ok(output)
}

fn bounded(mut hits: Vec<Hit>, top_k: usize, bytes: usize) -> Result<Vec<Hit>> {
    ensure!(
        (1..=100).contains(&top_k) && (1..=1_000_000).contains(&bytes),
        "invalid context bounds"
    );
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    let mut remaining = bytes;
    let mut result = Vec::new();
    for hit in hits {
        if result.len() == top_k {
            break;
        }
        // Do not truncate facts/citations mid-chunk. Oversized chunks are omitted.
        if hit.text.len() <= remaining {
            remaining -= hit.text.len();
            result.push(hit);
        }
    }
    Ok(result)
}

/// BM25, k1=1.2/b=0.75, case-folded alphanumeric terms; parameters are frozen.
pub fn lexical(docs: &[Document], query: &str, top_k: usize, bytes: usize) -> Result<Vec<Hit>> {
    let query: BTreeSet<_> = terms(query).into_iter().collect();
    let tokenized: Vec<_> = docs.iter().map(|d| terms(&d.text)).collect();
    let average = tokenized.iter().map(Vec::len).sum::<usize>() as f64 / docs.len().max(1) as f64;
    let mut frequencies = BTreeMap::new();
    for tokens in &tokenized {
        for token in tokens.iter().collect::<BTreeSet<_>>() {
            *frequencies.entry(token).or_insert(0usize) += 1;
        }
    }
    let hits = docs
        .iter()
        .zip(&tokenized)
        .filter_map(|(doc, tokens)| {
            let mut score = 0.0;
            for term in &query {
                let tf = tokens.iter().filter(|t| *t == term).count() as f64;
                if tf == 0.0 {
                    continue;
                }
                let df = *frequencies.get(term).unwrap_or(&0) as f64;
                let idf = (1.0 + (docs.len() as f64 - df + 0.5) / (df + 0.5)).ln();
                score += idf * tf * 2.2
                    / (tf + 1.2 * (0.25 + 0.75 * tokens.len() as f64 / average.max(1.0)));
            }
            (score > 0.0).then(|| Hit {
                id: doc.id.clone(),
                text: doc.text.clone(),
                score,
            })
        })
        .collect();
    bounded(hits, top_k, bytes)
}

/// Caller supplies embeddings from the locked model; fake vectors are legal only
/// in offline harness tests. Reject NaN, zero and mismatched vectors explicitly.
pub fn vector(
    docs: &[Document],
    embeddings: &[Vec<f64>],
    query: &[f64],
    top_k: usize,
    bytes: usize,
) -> Result<Vec<Hit>> {
    ensure!(
        docs.len() == embeddings.len() && !query.is_empty(),
        "embedding count/dimension mismatch"
    );
    let norm = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
    let qnorm = norm(query);
    ensure!(
        query.iter().all(|v| v.is_finite()) && qnorm.is_finite() && qnorm > 0.0,
        "invalid query vector"
    );
    let mut hits = Vec::new();
    for (doc, values) in docs.iter().zip(embeddings) {
        let dnorm = norm(values);
        ensure!(
            values.len() == query.len()
                && values.iter().all(|v| v.is_finite())
                && dnorm.is_finite()
                && dnorm > 0.0,
            "invalid document vector"
        );
        let score = values
            .iter()
            .zip(query)
            .map(|(a, b)| (a / dnorm) * (b / qnorm))
            .sum::<f64>();
        hits.push(Hit {
            id: doc.id.clone(),
            text: doc.text.clone(),
            score,
        });
    }
    bounded(hits, top_k, bytes)
}

/// Indexed handover pack, in stable source order. It is prepared independently
/// of questions, not a query-specific summary or a disguised Memory retriever.
pub fn handover(documents: &[Document], bytes: usize) -> Result<Vec<Hit>> {
    ensure!(documents.len() <= 100, "handover pack too large");
    let size: usize = documents.iter().map(|d| d.text.len()).sum();
    ensure!(
        size <= bytes,
        "handover pack exceeds common context budget; do not silently truncate"
    );
    bounded(
        documents
            .iter()
            .map(|d| Hit {
                id: d.id.clone(),
                text: d.text.clone(),
                score: 0.0,
            })
            .collect(),
        100,
        bytes,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparators_preserve_sources_and_apply_identical_bounds() {
        let docs = vec![
            Document {
                id: "a".into(),
                text: "retry queue".into(),
            },
            Document {
                id: "b".into(),
                text: "retention window".into(),
            },
        ];
        assert_eq!(lexical(&docs, "retry", 1, 100).unwrap()[0].id, "a");
        assert_eq!(
            vector(
                &docs,
                &[vec![1.0, 0.0], vec![0.0, 1.0]],
                &[0.0, 1.0],
                1,
                100
            )
            .unwrap()[0]
                .id,
            "b"
        );
        assert_eq!(handover(&docs, 100).unwrap().len(), 2);
        assert!(handover(&docs, 1).is_err());
        assert!(vector(&docs, &[vec![0.0], vec![1.0]], &[1.0], 1, 100).is_err());
        assert!(vector(&docs, &[vec![1.0], vec![1.0]], &[f64::NAN], 1, 100).is_err());
    }
    #[test]
    fn chunks_are_stable_and_unicode_safe() {
        let docs = vec![Document {
            id: "runbook".into(),
            text: "é".repeat(257),
        }];
        let result = chunks(&docs, 128).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[2].id, "runbook#2");
        assert_eq!(result[2].text, "é");
    }
}
