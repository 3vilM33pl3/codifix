# R3 coding failure diagnosis — 2026-09-17

## Conclusion

The recorded **0/15 sequence passes are not a clean measure of coding quality or
Memory's value**. The live harness combines strict agent completion with hidden
code tests, and its conservative deadline policy frequently prevented completion.
Missing agent-side tooling consumed additional calls. There are also genuine
failed submissions; not every failure is attributable to the harness.

This investigation used the frozen continuation-5 artifacts and offline,
network-disabled containers only. No paid model calls, source-code fixes, changes
to labels/scoring, new human ballots or retrospective score changes were made.

## Evidence

### 1. The five-minute budget permits much less iterative work

`runtime/isolation/rehearsal-task.mjs:219` subtracts 100 seconds from the
300-second task deadline for grading/finalization. Before each model call,
`runtime/isolation/model-agent.mjs:42` additionally requires a full 120-second
model window plus the 30-second command window to remain. Consequently, another
call cannot start after roughly **50 seconds** of elapsed work; retrieval/setup
and between-step grading reduce that further. A call started earlier can finish
later, so this is an admission threshold, not a hard 50-second task timeout.

An offline fake-clock reproduction using the exact frozen agent confirms that
after one 60-second finish response, the next step is refused with **240 seconds
left in the outer task deadline**: 140 seconds remain in the agent budget, less
than the required 150-second window. No network or real sleep was needed.

Observed live records:

- All 15 tasks were granted 12 provider calls; they used only 1–5, including
  retrieval. This was not a per-task or campaign call-count exhaustion.
- **21/45 steps received zero model calls**: eight second steps and thirteen
  third steps. The call counts and frozen control flow identify the time guard
  as the stop condition for those zero-call steps.
- Entire executor durations were approximately **59–109 seconds**, not five
  minutes. Measured private grading was approximately ten seconds per sequence;
  that observation alone is not justification to remove safe worst-case bounds.

### 2. Correct code can still have a failed protocol outcome

The sequence runner requires both agent exit zero (an explicit `finish` action)
and a successful private grade. An exhausted-window return has agent exit one,
even if it returns working source. The final observation discards the separate
agent/grader exit statuses and termination reason, retaining a combined boolean.

We replayed **all 24 recorded commands**, in their original per-step order, in
the pinned network-disabled Rust containers. We did not generate new actions or
edit their submitted code. Recorded commands do not read the context/task file;
the replay supplies the current public prompt with empty retrieval context and
then requires the reconstructed grader-input hash to match the original before
accepting any replay evidence. **All 45 grader-input hashes matched**, binding
the exact selected source bytes and hidden probes to the original observations.

Re-running those probes found:

| Step outcome | Count |
| --- | ---: |
| Original combined agent-and-grader passes | 8 |
| Code-only probe passes in exact-source replay | 37 |
| Code passes but original combined outcome failed | 29 |
| Actual code/probe failures | 8 |

All 21 zero-call steps already had source that passed their code probes. Eight
additional steps had model activity and passing source but no accepted `finish`.
Ten of the fifteen saved trajectories pass every stage's code probes in replay.
**That is diagnostic evidence, not permission to change the recorded 0/15 to
10/15**, or a new independent evaluation. The frozen strict completion criterion
remains the original scoring contract.

### 3. Agent tool affordances do not match common Rust commands

The pinned guest exposes `/usr/local/bin/rustc` and `/usr/bin/python3`, but neither
`cargo` nor `python`. The hidden grader knows the explicit `rustc` path. The
model's tool description does not advertise these executable paths or a verified
public build command. Its fixture contains `Cargo.toml`, which makes Cargo-based
commands unsurprising but not executable in this image.

Offline replay produced **18 nonzero command exits out of 24**, including missing
Cargo, missing `python`, and direct `/usr/bin/cargo` requests. Several commands
wrote working source before their attempted test command failed. Returning these
as bounded tool feedback was correct, but the tight time guard often prevented
the agent from recovering or sending its finish action.

### 4. Some failures are genuine model/submission failures

Eight steps failed the actual code probes despite an explicit finish response.
Seven of these had no preceding command in that step; the other followed failed
tool commands. Five sequences include at least one finish-without-edit action.
We cannot infer that changing the clock or installing Cargo would fix these.

## Recommended correction before another coding calibration

These are recommendations, not implemented changes or approval to run again:

1. Specify one coherent whole-sequence time budget with measured, bounded grading
   headroom and a feasible opportunity for all three stages. Test realistic
   multi-call latencies offline before freezing a revised runtime/configuration.
2. Advertise verified executable paths and a public build/test command available
   in the pinned image. Either deliberately retain a rustc-only environment or
   pin a Cargo-capable image; do not silently change it during a campaign.
3. Persist separate agent termination reason, agent exit, code-test outcome and
   consumed budget. Distinguish budget exhaustion from incorrect code without
   weakening or retroactively changing the frozen success rule.
4. Add offline cases where source is correct but finish is prevented, where later
   steps get no invocation, and where tool lookup fails. Retain this run and its
   billing evidence; a later revised run gets its own identity and documented
   operational-budget decision.

The knowledge review remains a separate valid next action: the normal blinded
export already succeeded. Do not infer its judgments from this coding diagnosis.

## Reproduction and preservation

- Source campaign: `/data/tmp/r3-continue5-20260917`, manifest
  `b43cd0958100064339d6d37e7a84534be1eae4d1f071b22b250782903ee979f7`.
- Diagnostic script: `/tmp/r3-coding-diagnose.mjs`.
- Per-command and per-step evidence: `/tmp/r3-coding-diagnosis.json` and
  `/tmp/r3-coding-diagnosis.log`.
- All replay containers were automatically removed; none remain. Frozen runtime,
  original observations, provider receipts and blank review ballots are unchanged.

R3 remains incomplete. This is an offline diagnosis, not a fix or a passing
benchmark. No additional evaluation-provider spending occurred.
