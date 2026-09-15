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

fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = vec![];
    fs::File::open(path)?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 2 * 1024 * 1024, "oversized review input");
    Ok(serde_json::from_slice(&bytes)?)
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
    ensure!(
        args.len() == 3,
        "expected prepare INPUT NEW_DIRECTORY or score EVIDENCE BALLOTS"
    );
    match args[0].to_str() {
        Some("prepare") => {
            let input: Input = read(Path::new(&args[1]))?;
            let evidence = KnowledgeEvidence::prepare(
                input.manifest_sha256,
                &input.work,
                input.rubric,
                input.sources,
                input.answer,
            )?;
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
