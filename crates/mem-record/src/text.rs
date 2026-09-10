// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared pure lexical/excerpt helpers extracted from mem-search. No repository
//! lookup, project paths, ranking configuration or writeback enters this module.
use std::collections::HashSet;

/// Tokens that carry no topical content in the existing deterministic answerer.
pub fn is_stopword(term: &str) -> bool {
    const STOPWORDS: &[&str] = &[
        "the", "a", "an", "is", "are", "was", "were", "be", "for", "of", "on", "in", "to", "and",
        "or", "as", "at", "by", "with", "since", "after", "before", "it", "its", "that", "this",
        "not", "no", "how", "what", "when", "where", "why", "who", "which", "do", "does", "did",
        "can", "could", "should", "would", "will", "we", "you", "your", "our", "my", "me",
    ];
    STOPWORDS.contains(&term)
}
pub fn extract_lexical_terms(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    query
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-')))
        .filter_map(|raw| {
            let term = raw.trim().to_lowercase();
            if term.len() < 2 || !seen.insert(term.clone()) {
                None
            } else {
                Some(term)
            }
        })
        .collect()
}
/// The caller supplies normalized text and terms, as in mem-search.
pub fn lexical_overlap_ratio(text: &str, terms: &[String]) -> f64 {
    if terms.is_empty() {
        return 0.0;
    }
    let matched = terms
        .iter()
        .filter(|term| text.contains(term.as_str()))
        .count();
    matched as f64 / terms.len() as f64
}
pub fn summarize_snippet(text: &str, lexical_terms: &[String], phrases: &[String]) -> String {
    let trimmed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.len() <= 240 {
        return trimmed;
    }
    let lowered = trimmed.to_lowercase();
    let target = phrases
        .iter()
        .map(|value| value.to_lowercase())
        .chain(lexical_terms.iter().cloned())
        .find_map(|needle| lowered.find(&needle));
    if let Some(index) = target {
        let start = floor_char_boundary(&trimmed, index.saturating_sub(80));
        let end = floor_char_boundary(&trimmed, (start + 240).min(trimmed.len()));
        let prefix = if start > 0 { "..." } else { "" };
        let suffix = if end < trimmed.len() { "..." } else { "" };
        return format!("{prefix}{}{suffix}", &trimmed[start..end]);
    }
    let end = floor_char_boundary(&trimmed, 240);
    format!("{}...", &trimmed[..end])
}
fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_normalization_matching_and_snippets_are_stable() {
        assert_eq!(
            extract_lexical_terms("How does DNS work? DNS /etc/config a"),
            vec!["how", "does", "dns", "work", "/etc/config"]
        );
        assert!(is_stopword("does"));
        assert_eq!(
            lexical_overlap_ratio("dns server", &["dns".into(), "zones".into()]),
            0.5
        );
        assert_eq!(
            summarize_snippet("  DNS\n servers  ", &[], &[]),
            "DNS servers"
        );
        let long = "é".repeat(300);
        assert_eq!(
            summarize_snippet(&long, &[], &[]),
            format!("{}...", "é".repeat(120))
        );
    }
}
