// SPDX-License-Identifier: AGPL-3.0-or-later
//! Trusted-host review utility; never mount its inputs/outputs in answering agents.
use anyhow::{Result, ensure};
use mem_eval::campaign::{
    Work,
    scoring::{Answer, Ballot, KnowledgeEvidence, Rubric, Source, assess},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    manifest_sha256: String,
    work: Work,
    rubric: Rubric,
    sources: Vec<Source>,
    answer: Answer,
}

const INPUT_LIMIT: u64 = 2 * 1024 * 1024;

fn read_bounded<T: DeserializeOwned>(reader: impl Read) -> Result<T> {
    let mut bytes = vec![];
    reader.take(INPUT_LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= INPUT_LIMIT, "oversized review input");
    Ok(serde_json::from_slice(&bytes)?)
}

fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    read_bounded(fs::File::open(path)?)
}

fn prepare(input: Input) -> Result<KnowledgeEvidence> {
    KnowledgeEvidence::prepare(
        input.manifest_sha256,
        &input.work,
        input.rubric,
        input.sources,
        input.answer,
    )
}

/// Trusted-host, bounded pipe assessment. No rubric/evidence temporary files or
/// caller-supplied ballots: semantic judgments deliberately remain unresolved.
fn assess_stdin(reader: impl Read, writer: impl Write) -> Result<()> {
    let evidence = prepare(read_bounded(reader)?)?;
    let assessment = assess(&evidence.packet, &[])?;
    serde_json::to_writer(
        writer,
        &serde_json::json!({ "assessment": assessment, "evidence": evidence }),
    )?;
    Ok(())
}

fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    Ok(())
}

fn directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?; // Existing output is never overwritten or resumed.
    Ok(())
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "assess-stdin" {
        return assess_stdin(std::io::stdin().lock(), std::io::stdout().lock());
    }
    ensure!(
        args.len() == 3,
        "expected assess-stdin, prepare INPUT NEW_DIRECTORY or score EVIDENCE BALLOTS"
    );
    match args[0].to_str() {
        Some("prepare") => {
            let input: Input = read(Path::new(&args[1]))?;
            let evidence = prepare(input)?;
            let out = Path::new(&args[2]);
            directory(out)?;
            directory(&out.join("private"))?;
            directory(&out.join("review"))?;
            write(&out.join("private/evidence.json"), &evidence)?;
            write(&out.join("review/packet.json"), &evidence.packet)?;
            write(
                &out.join("review/ballot.json"),
                &evidence.packet.blank_ballot("pending-reviewer".into())?,
            )?;
            println!("Prepared blinded review; human decisions remain pending.");
        }
        Some("score") => {
            let mut evidence: KnowledgeEvidence = read(Path::new(&args[1]))?;
            ensure!(
                evidence.ballots.is_empty() && evidence.history.is_empty(),
                "use the original unadjudicated evidence; do not replace prior ballots"
            );
            let ballots: Vec<Ballot> = read(Path::new(&args[2]))?;
            let assessment = assess(&evidence.packet, &ballots)?;
            evidence.ballots = ballots;
            // A valid negative or pending review is data, not a tool failure.
            // Campaign validation separately rejects unresolved adjudication.
            serde_json::to_writer(
                std::io::stdout(),
                &serde_json::json!({ "assessment": assessment, "evidence": evidence }),
            )?;
        }
        _ => anyhow::bail!("unknown review operation"),
    }
    Ok(())
}

fn main() {
    if run().is_err() {
        eprintln!("R3 scoring rejected arguments or review evidence");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> serde_json::Value {
        let criteria = ["factual_correctness", "required_evidence", "appropriate_refusal"].into_iter().enumerate().map(|(i, dimension)|
            serde_json::json!({ "id": format!("criterion-{i}"), "dimension": dimension, "instruction": "Private human review criterion", "critical": false })).collect::<Vec<_>>();
        serde_json::json!({
            "manifest_sha256": "a".repeat(64),
            "work": { "case_id": "pipe-case", "condition": "lexical", "repeat": 0 },
            "rubric": { "case_id": "pipe-case", "question": "What is retained?",
                "criteria": criteria,
                "minimum_citations": 1, "citations_critical": true },
            "sources": [{ "id": "public/source.md", "text": "History is retained." }],
            "answer": { "text": "History is retained.", "refused": false,
                "citations": [{ "source_id": "public/source.md", "quote": "History is retained." }] }
        })
    }

    #[test]
    fn stdin_assessment_preserves_pending_human_decisions() {
        let mut output = vec![];
        assess_stdin(
            serde_json::to_vec(&input()).unwrap().as_slice(),
            &mut output,
        )
        .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(result["assessment"]["unresolved"], true);
        assert_eq!(result["assessment"]["success"], false);
        assert_eq!(result["assessment"]["citations_valid"], true);
        assert_eq!(result["evidence"]["ballots"], serde_json::json!([]));
        assert_eq!(result["evidence"]["history"], serde_json::json!([]));
        assert_eq!(result["evidence"]["packet"]["sources"], input()["sources"]);
    }

    #[test]
    fn stdin_invalid_citation_is_negative_evidence_not_a_tool_error() {
        let mut value = input();
        value["answer"]["citations"][0]["quote"] = "Not present in source".into();
        let mut output = vec![];
        assess_stdin(serde_json::to_vec(&value).unwrap().as_slice(), &mut output).unwrap();
        let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(result["assessment"]["citations_valid"], false);
        assert_eq!(result["assessment"]["critical_failure"], true);
        assert_eq!(result["assessment"]["success"], false);
    }

    #[test]
    fn stdin_rejects_oversize_unknown_fields_and_mismatched_work_without_output() {
        let mut output = vec![];
        assert!(assess_stdin(std::io::repeat(b' ').take(INPUT_LIMIT + 1), &mut output).is_err());
        assert!(output.is_empty());
        let mut unknown = input();
        unknown["extra"] = true.into();
        assert!(
            assess_stdin(
                serde_json::to_vec(&unknown).unwrap().as_slice(),
                &mut output
            )
            .is_err()
        );
        assert!(assess_stdin(b"not JSON".as_slice(), &mut output).is_err());
        assert!(output.is_empty());
        let mut mismatch = input();
        mismatch["work"]["case_id"] = "other-case".into();
        assert!(
            assess_stdin(
                serde_json::to_vec(&mismatch).unwrap().as_slice(),
                &mut output
            )
            .is_err()
        );
        assert!(output.is_empty());
    }
}
