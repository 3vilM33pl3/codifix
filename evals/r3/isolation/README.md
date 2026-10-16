# R3 offline container boundary

## Explicit native Windows sequence integration

`sequence.mjs` now uses `execution.mjs` for command-grader sequences. Existing
callers without an `execution` field keep the unchanged Linux container backend
and image-labelled result format. Windows is selected only by a reviewed
`execution: { kind: "windows-native-v1", profile, profile_sha256 }` contract;
`nativeProfileHash(profile)` computes its canonical digest. It has no Docker
`image` field and does not translate POSIX executables or `/work` paths.

The profile binds the immutable, operator-installed native helper, pinned
Node/guest files, complete toolchain file inventory, search/library directories
and an existing private I/O root. The native helper validates that root and
creates a private invocation directory **before** JavaScript writes payloads.
JavaScript file modes and fake-helper tests are not Windows ACL evidence.

Pass explicit Windows `argv` values in all three steps and their command graders.
Every agent command and grader receives a fresh helper invocation. Only selected
source exports move between steps; grader files, outputs and future-step context
do not. Missing required source remains a failed step even if a later step repairs
it. Native results carry the profile digest and per-invocation descriptor/receipt
digests, are marked `officialEvidence: false`, and explicitly report no hard disk
quota. They are not interchangeable with Linux image evidence.

Native Rust-blackbox grading uses the same execution seam. Supply
`compiler: { path, sha256 }` (or `step.grader.compiler` in a sequence), matching
an exact `rustc.exe` entry in the reviewed native toolchain inventory. Each
probe compiles and runs in fresh bounded scratch; only probe arguments/stdin,
not expected output, enter the grader. Readiness and per-probe receipts remain
in diagnostics. The Linux harness and image-labelled artifacts stay unchanged.

Model-command execution, knowledge adjudication and full campaign orchestration
remain separate work. Their entrypoints still reject explicit native requests
before execution rather than silently falling back to Linux. Native support is **not** full Windows R3
completion. Paid R3 remains paused; real Windows launch, compilation, denial and
recovery evidence is still required.

Focused protocol/sequence checks (fake executors, no native-isolation claim):

```bash
node --test evals/r3/isolation/native-windows.test.mjs \
  evals/r3/isolation/sequence-native.test.mjs \
  evals/r3/isolation/rust-native.test.mjs \
  evals/r3/isolation/sequence.test.mjs
```

## Representative task executor

`rehearsal-task.mjs` composes a prepared service session, existing baseline
executable, isolated synthetic answers or three-step Rust grading, and the
existing campaign observation shape. It accepts only a draft zero-spend smoke
pack and verifies all input hashes before and after work. Each condition gets a
fresh clone of the same prepared snapshot. All knowledge conditions are exercised
with real sources; coding no-op controls fail real private acceptance tests.

The fake vector function and excerpt/no-op agents are harness controls, not model
or ranking quality evidence. Rehearsal duration currently includes disposable
service setup and hidden grading and is explicitly not model/user latency.
Host-side service orchestration remains trusted; answering/coding containers have
no network. The hash-locked offline controller/CLI driver is now available through
`../rehearse.mjs`; official preparation parity and paid provider mediation remain
unfinished. The full rehearsal test replaces a test-owned upstream build output
mid-run and verifies continued use of the privately copied executable.

Sequence steps may supply `inputFiles` for the current prompt/context. They cannot
overwrite fixture/submission paths. Inputs are validated for every stage before
execution, but only the current stage's files enter its process; they do not carry
over with the allowlisted source submission. Tests verify that future stage files
and earlier stage-only files are absent and prepared context cannot be replaced.

## Knowledge answer and adjudication bridge

`knowledge.mjs` connects explicitly selected prepared source chunks to an offline
answer container and the existing `r3_score` executable. `preparedContext` checks
baseline source bytes or exact Memory source/version/hash/project bindings. It
does not forward generated service answers, snippets, paths or debug metadata.
Selection retains retrieval order and omits whole chunks exceeding the common
32-KiB/eight-result ceiling; official chunking/preparation parity remains open.

`runOfflineKnowledge` supplies only `input.json` containing question and selected
context. The approved synthetic agent command must write a bounded structured
`answer.json`; no rubric, unselected source, credentials or condition label enters
the container. Full source material and labels remain with the host adjudicator.
The resulting evidence has no human ballots. Exact valid quotes do not establish
semantic correctness, and pending judgments never pass. Invalid citations remain
negative scoring evidence; malformed process output fails execution explicitly.

This is offline-only and does not invoke or account for a model. Answer execution
and adjudication-preparation timings are separate; neither is represented as
whole-task latency including retrieval. The caller must preserve evidence and
measure that outer interval. Hash-locked campaign dispatch, paid mediation,
embeddings, complete datasets and official execution are still separate work.

The required knowledge test runs actual document/lexical/offline-vector adapters,
isolated synthetic answering, no-memory refusal and Rust adjudication. The
required snapshot test additionally runs real sealed Memory HTTP retrieval,
source binding and isolated answering, then rechecks the database fingerprint.
Both CI gates use disposable data and the pinned Node image described below.

## Container contract

`service-session.mjs` owns one prepared snapshot and starts fresh scoped service
clones through `withInstance(project, callback)`. Its callback receives only
opaque instance/source bindings and a fixed-project deterministic query function,
not database credentials or a service token. The query function stops accepting
calls when that callback ends. Each callback gets a different read-only clone;
the host checks its fingerprint before and after work, including callback errors.
Services stop before clone release. Tests demonstrate independent resets, an
interrupted callback followed by successful recovery, and isolated answering.

The service processes are trusted host-side test helpers, not the agent boundary.
Their environment and configuration are isolated from existing deployments. The
caller must supply an explicitly disposable database. Normal session close
removes only owned clones/configuration; an ambiguous owner exit retains private
staging and requires reconciliation. No crash-safe controller or production
snapshot/recovery claim is implied. Do not pass this session to an agent.

This is a reusable execution component for the existing campaign driver, **not a
complete driver or evidence that R3 is finished**. It executes explicitly supplied
synthetic commands without network or provider access. It does not select context,
prepare PostgreSQL snapshots, run Memory retrieval, authorize labels
or mediate paid calls. The earlier shared Docker eval stacks remain unsuitable
for R3 isolation and are not modified by this component.

Live coding shares the whole-task request budget with retrieval and uses a
monotonic cutoff that includes time between steps. It reserves 100 seconds for
grading/final verification and requires a full provider-plus-command window before
starting another model call. A budget-exhausted agent returns failed submissions
with known usage; neither it nor later untouched steps become successful. This
conservative allocation reduces usable agent time and must accompany development
results. It does not turn uncertain provider calls or isolation failures into
valid observations. Offline synthetic agents retain their existing test budgets.

`runOfflineContainer({ image, argv, files, timeoutMs, exportPaths })` accepts an immutable local
Docker image ID, an absolute executable with argument array, and a map of explicit
relative filenames to UTF-8 contents. There is no host source-directory mount or
inherited environment option. Defaults: 10-second command deadline, maximum
60 seconds, 1 MiB input/output contents, 256 files, 256 MiB memory and 64 PIDs.
The image must supply `/usr/bin/env` and `/usr/local/bin/node`, contain no implicit
volumes, and be reviewed separately. Mutable image tags and automatic pulls are
rejected by the component.

An unavailable or non-executable guest command (spawn `ENOENT`/`EACCES`) returns
exit 127/126, a fixed bounded diagnostic and the selected files. This is task
feedback, not a host fallback or a successful task. A model may correct its next
command only within its remaining task/call budget. Other startup failures,
timeouts, excessive output, unsafe exports and supervisor/container failures
continue to abort execution. Raw spawn errors are not exposed.

Optional `exportPaths` is an exact list of relative regular-file paths (at most
256, no duplicates or ancestor conflicts). Omission preserves bounded whole-tree
export; an empty list exports no files. Selected missing files remain missing.
Only ancestors of selected files are traversed; build products outside the list
are not exported. Selected symlinks, hard links, special files and directories
fail closed. The host rejects unsolicited returned files. The invocation contract
is copied before the first await, preventing caller mutation during staging.

Each invocation gets a new non-root container, read-only root filesystem, no
network, all capabilities dropped, no-new-privileges, disabled healthchecks and
fresh 16 MiB `/work` and `/tmp` tmpfs mounts. The only host mounts are a private,
read-only staging directory containing the supplied inputs and the read-only
guest supervisor. No Docker socket, host home, credentials, plugins or hidden
grader directory is mounted. Image environment variables are cleared before
starting Node; evaluated commands receive only a minimal PATH, temporary HOME
and locale. The host Docker client is fixed to the local Unix socket.

Return values contain exit code, bounded stdout/stderr, base64 regular-file
submissions, elapsed time and image identity. File collection rejects symlinks,
hard links and special files; the host revalidates names, encodings and bounds.
Treat **all** returned content and exit status as untrusted submission data, not
a correctness verdict or an isolation attestation. The same-UID command may
interfere with its supervisor; an independent grader must decide correctness.
No file archive is extracted onto the host.

For a coding sequence, the future driver may explicitly carry validated submission
files into the next step of that sequence. Unrelated cases/conditions must start
from their original prepared files. The test demonstrates this transfer and a
fresh reset. The Rust profile below supplies executable bounded build scratch;
this component does not yet run a complete maintenance benchmark.

Normal completion, nonzero supervisor exit, excess output and timeout remove only
the invocation's uniquely named container and staging directory. Cleanup ambiguity
reports the exact name/path and retains staging. Host SIGKILL or daemon failure
can leave an orphan with label `memory.r3.disposable=true`; inspect its exact
ownership before removal. Automatic interrupted-campaign reconciliation remains
driver work. No broad container prune or removal of another workload is allowed.
Docker/kernel/host administrators remain trusted; this is not a VM boundary.

## Verification

Pure validation tests require Node only; the live test is explicitly skipped
unless an image is supplied. A required live gate must also set
`MEMORY_R3_REQUIRE_CONTAINER=1`, which rejects missing image configuration.

```bash
MEMORY_R3_REQUIRE_CONTAINER=1 \
MEMORY_R3_TEST_IMAGE=sha256:ae7d65bc9a8e0747b9d1a2e8aa8da051e1bb65dda859064657307c355e3dbdc7 \
node --test evals/r3/isolation/container.test.mjs
```

The tested local x86-64 image is the repository's pinned PDS fixture image
(`ghcr.io/bluesky-social/pds@sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12`).
Only its Node runtime runs: no PDS service, accounts or records are started. CI
pulls that immutable fixture explicitly, resolves its local ID and runs this gate
in the offline-eval job. Local execution used the cached image and no registry or
paid provider request. This is not a recommended production agent image.

Live checks exercise unavailable hidden host files, minimal environment, Docker
socket denial, read-only inputs/root, loopback-only networking, selected-context
availability, explicit sequence transfer, clean independent resets, symlink
rejection, output flooding and timeout cleanup. Pure tests reject traversal,
conflicting paths, mutable images, excessive inputs and forged output metadata.
These tests do not prove service snapshot sealing, cross-condition retrieval,
production coding-grader execution or provider-budget mediation. Those R3 gates remain open.

## Three-step agent/grader handoff

`runOfflineSequence` in `sequence.mjs` validates an explicit three-step contract
before dispatch. Inputs are `image`, `initialFiles`, `submissionPaths`, three
`steps` (`id`, agent `argv`, and private `grader: { argv, files }`), and optional
`timeoutMs`. All submission paths must be declared in the initial fixture. It
clones the contract before its first asynchronous operation, so later caller
mutation cannot replace a grader or subsequent command mid-run.

Each step launches one agent container and, after it exits, a different grader
container. The agent receives public files and the selected submission from the
previous step, never grader files or feedback. The grader receives only selected
agent bytes under `submission/` and trusted checks under `grader/`. These separate
namespaces prevent an agent-written fake grader from replacing the real checks.
Only selected agent files advance; modifications made by the grader are discarded.
Agent containers export only `submissionPaths`; grader containers export no files.
Non-submission public inputs are restored from the original fixture every step.
A deleted selected file stays missing until the agent deliberately recreates it.

Results contain per-step exit codes, diagnostic timings and input/submission
hashes, plus the final selected submission and whole-sequence elapsed time. They
contain no grader output/feedback. Whole-sequence success requires all three
steps, including required-file presence; a later repair does not erase an earlier
failure. A nonzero grading result is valid negative evidence, not an infrastructure
error. Container failures still fail closed. These records explicitly say
`kind: offline-sequence-smoke` and `officialEvidence: false`; they are **not**
complete `mem-eval` observations or approved benchmark evidence. Steps must not be
counted as independent campaign samples.

The original live test uses trusted Node checks over inert JSON submissions. It proves
the handoff, hidden-feedback exclusion, fake-grader exclusion, positive/negative
controls, deleted-file carryover and fresh sequence reset. It does not yet grade
untrusted compiled Rust programs or claim resistance to attacks on such graders.
Existing suite runner integration and artifact mapping remain unfinished. No new representative
benchmark labels or paid execution are enabled.

## Dependency-free Rust black-box grading

`Dockerfile.rust` combines digest-pinned official Rust 1.98.1 and Node 22.19.0
images. It copies no checkout, host configuration or assistant. Build with:

```bash
docker build --network=none -f evals/r3/isolation/Dockerfile.rust -t memory-r3-rust:offline evals/r3/isolation
r3_rust_image=$(docker image inspect --format '{{.Id}}' memory-r3-rust:offline)
MEMORY_R3_RUST_IMAGE="$r3_rust_image" MEMORY_R3_REQUIRE_RUST=1 \
  node --test evals/r3/isolation/rust.test.mjs
```

Image acquisition may access registries; build commands and grading have no
network access. The fixed `rust` execution profile allows 512 MiB memory,
128 MiB executable work tmpfs and 32 MiB per output file. `/tmp` remains
non-executable and limited to 16 MiB. Default text executions retain their
smaller limits and non-executable scratch. All input/export bounds remain.

`gradeRustCase({ image, files, probes, timeoutMs })` compiles a dependency-free
binary rooted at `src/main.rs`, including ordinary Rust modules. Each probe
has an ID, argv, stdin and exact expected stdout. Expected outputs remain in
the host grader, never the execution container. Probe inputs are necessarily
visible to the program while it executes. An independent reference compilation
and execution checks toolchain availability before submitted code is graded.
Compilation errors and incorrect results are measured failures; unavailable
tooling/launch infrastructure fails the measurement. No Cargo build scripts,
dependency resolution or submitted test harness is used.

Sequence steps select this with `grader: { kind: 'rust-blackbox', probes }`.
The agent receives neither probes nor expectations; fresh grading containers
receive selected source and probe inputs only. No compiler/program output is
returned to subsequent agents. All three steps must pass. Tests exercise real
multi-file Rust, a falsely successful program, invalid code, private expectation
exclusion, source carryover and a failed middle step followed by repair.
These are smoke checks, not representative tasks or official evaluation.
This provides black-box executable testing, not general Cargo unit-test support.

Run all component tests with the same explicitly required image configuration:

```bash
MEMORY_R3_REQUIRE_CONTAINER=1 \
MEMORY_R3_TEST_IMAGE=sha256:ae7d65bc9a8e0747b9d1a2e8aa8da051e1bb65dda859064657307c355e3dbdc7 \
node --test evals/r3/isolation/*.test.mjs
```
