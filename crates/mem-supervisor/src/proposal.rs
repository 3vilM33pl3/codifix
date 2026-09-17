//! Private model suggestions, never executable instructions or approval tokens.
//! Validation establishes shape and reference bounds, not factual truth or
//! permission to disclose/act on the supplied text.

use serde::{Deserialize, Serialize};

pub const MAX_SUMMARY_BYTES: usize = 4096;
pub const MAX_EVIDENCE: usize = 8;

/// Intentionally no Debug: operational logs must not render private text.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub summary: String,
    pub proposal: ProposalKind,
    pub evidence_indices: Vec<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    None,
    AskHuman,
    ProposeCorrection,
    ReviewRelease,
}

impl Proposal {
    pub fn validate(&self, evidence_count: usize) -> Result<(), InvalidProposal> {
        if self.summary.trim().is_empty()
            || self.summary.len() > MAX_SUMMARY_BYTES
            || evidence_count > MAX_EVIDENCE
            || self.evidence_indices.len() > MAX_EVIDENCE
        {
            return Err(InvalidProposal);
        }
        let mut seen = [false; MAX_EVIDENCE];
        for &index in &self.evidence_indices {
            if index >= evidence_count || seen[index] {
                return Err(InvalidProposal);
            }
            seen[index] = true;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("proposal exceeds private text or evidence-reference bounds")]
pub struct InvalidProposal;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_bounded_unique_supplied_references_without_conferring_authority() {
        let mut p = Proposal {
            summary: "Ask a human to verify the reviewed note".into(),
            proposal: ProposalKind::AskHuman,
            evidence_indices: vec![1, 0],
        };
        assert!(p.validate(2).is_ok());
        assert!(p.validate(1).is_err());
        assert!(p.validate(9).is_err());
        p.evidence_indices = vec![0, 0];
        assert!(p.validate(2).is_err());
        p.evidence_indices = vec![usize::MAX];
        assert!(p.validate(2).is_err());
        p.evidence_indices.clear();
        assert!(p.validate(0).is_ok());
        p.summary = " ".into();
        assert!(p.validate(0).is_err());
        p.summary = "é".repeat(2049);
        assert!(p.validate(0).is_err());
    }

    #[test]
    fn wire_shape_cannot_carry_execution_or_approval_fields() {
        for input in [
            r#"{"summary":"x","proposal":"execute","evidence_indices":[]}"#,
            r#"{"summary":"x","proposal":"none","evidence_indices":[],"approved":true}"#,
            r#"{"summary":"x","summary":"y","proposal":"none","evidence_indices":[]}"#,
        ] {
            assert!(serde_json::from_str::<Proposal>(input).is_err());
        }
        let p: Proposal = serde_json::from_str(
            r#"{"summary":"x","proposal":"review_release","evidence_indices":[]}"#,
        )
        .expect("bounded suggestion");
        assert_eq!(p.proposal, ProposalKind::ReviewRelease);
        assert!(p.validate(0).is_ok());
    }
}
