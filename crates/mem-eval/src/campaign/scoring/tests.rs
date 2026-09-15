// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

fn packet() -> ReviewPacket {
    ReviewPacket {
        version: 1,
        review_id: "opaque-review-123".into(),
        manifest_sha256: digest(b"campaign"),
        execution_sha256: digest(b"execution"),
        rubric: super::super::tests::rubric("knowledge-case"),
        sources: vec![Source {
            id: "source.md".into(),
            text: "The first settlement committed before acknowledgement failed.".into(),
        }],
        answer: Answer {
            text: "Acknowledgement failure does not establish that settlement failed.".into(),
            refused: false,
            citations: vec![Citation {
                source_id: "source.md".into(),
                quote: "settlement committed before acknowledgement failed".into(),
            }],
        },
    }
}

fn passing(packet: &ReviewPacket, reviewer: &str) -> Ballot {
    let mut ballot = packet.blank_ballot(reviewer.into()).unwrap();
    for judgment in ballot.judgments.values_mut() {
        judgment.decision = Decision::Pass;
        judgment.reason = "Synthetic human decision for unit test only".into();
    }
    ballot
}

#[test]
fn missing_and_model_only_judgments_never_pass() {
    let p = packet();
    let empty = assess(&p, &[]).unwrap();
    assert!(!empty.success && empty.unresolved);
    let blank = p.blank_ballot("reviewer".into()).unwrap();
    assert!(assess(&p, &[blank]).unwrap().unresolved);
    let mut diagnosis = passing(&p, "model");
    diagnosis.kind = ReviewerKind::ModelDiagnosis;
    let result = assess(&p, &[diagnosis]).unwrap();
    assert!(result.unresolved && !result.success);
    assert!(assess(&p, &[passing(&p, "human")]).unwrap().success);
}

#[test]
fn conflicts_are_unresolved_and_cannot_be_overwritten_by_ballot_order() {
    let p = packet();
    let yes = passing(&p, "first");
    let mut no = passing(&p, "second");
    no.judgments.get_mut("criterion-0").unwrap().decision = Decision::Fail;
    for ballots in [vec![yes.clone(), no.clone()], vec![no, yes.clone()]] {
        let result = assess(&p, &ballots).unwrap();
        assert!(result.unresolved && !result.success);
    }
    assert!(assess(&p, &[yes.clone(), yes]).is_err());
}

#[test]
fn every_decision_is_bound_to_exact_answer_rubric_sources_and_refusal() {
    let p = packet();
    let ballot = passing(&p, "human");
    let mut variants = vec![p.clone(); 5];
    variants[0].answer.text.push_str(" changed");
    variants[1].answer.refused = true;
    variants[2].sources[0].text.push_str(" changed");
    variants[3].rubric.criteria[0]
        .instruction
        .push_str(" changed");
    variants[4].review_id.push_str("-other");
    for changed in variants {
        assert!(assess(&changed, std::slice::from_ref(&ballot)).is_err());
    }
    let mut incomplete = ballot;
    incomplete.judgments.remove("criterion-0");
    assert!(assess(&p, &[incomplete]).is_err());
}

#[test]
fn fabricated_wrong_source_duplicate_and_missing_citations_fail_despite_human_pass() {
    let p = packet();
    let mut variants = vec![p.clone(); 5];
    variants[0].answer.citations[0].quote = "the payment never committed".into();
    variants[1].answer.citations[0].source_id = "other.md".into();
    variants[2]
        .answer
        .citations
        .push(p.answer.citations[0].clone());
    variants[3].answer.citations.clear();
    variants[4].answer.citations[0].quote = " ".into();
    for changed in variants {
        let result = assess(&changed, &[passing(&changed, "human")]).unwrap();
        assert!(!result.success && !result.citations_valid && result.critical_failure);
        assert!(!result.unresolved); // A valid measured negative, not missing data.
    }
}

#[test]
fn refusal_and_factuality_are_semantic_not_substring_tests() {
    let mut p = packet();
    p.answer.text = "The packet cannot establish the total loss.".into();
    p.answer.refused = true;
    let yes = passing(&p, "human");
    assert!(assess(&p, &[yes]).unwrap().success);
    p.answer.text = "The loss was exactly zero.".into();
    p.answer.refused = false;
    p.rubric.criteria[0].critical = true;
    let mut no = passing(&p, "human");
    no.judgments.get_mut("criterion-0").unwrap().decision = Decision::Fail;
    let result = assess(&p, &[no]).unwrap();
    assert!(!result.success && !result.unresolved && result.critical_failure);
}

#[test]
fn blinded_packet_has_no_condition_model_repeat_or_automatic_score() {
    let p = packet();
    let json = serde_json::to_value(&p).unwrap();
    for key in [
        "condition",
        "repeat",
        "model",
        "work",
        "success",
        "judgments",
    ] {
        assert!(json.get(key).is_none());
    }
    let mut invalid = p;
    invalid.rubric.criteria.pop();
    assert!(invalid.validate().is_err());
}

#[test]
fn representative_rubrics_use_canonical_locked_bytes_and_all_dimensions() {
    for family in [
        "supplier-handover",
        "turnover-recovery",
        "cache-partition",
        "schema-transition",
        "capacity-pressure",
        "timezone-window",
        "artifact-origin",
        "retention-hold",
    ] {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../evals/r3/representative/development/{family}/private"
        ));
        for index in 1..=4 {
            let bytes = std::fs::read(root.join(format!("q{index}.json"))).unwrap();
            let rubric: Rubric = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(rubric_bytes(&rubric).unwrap(), bytes);
            assert_eq!(rubric.case_id, format!("{family}-q{index}"));
            let mut p = packet();
            p.rubric = rubric;
            p.validate().unwrap();
            assert!(assess(&p, &[]).unwrap().unresolved);
        }
    }
}
