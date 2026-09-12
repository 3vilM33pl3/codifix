# R3 offline container boundary

This is a reusable execution component for the existing campaign driver, **not a
complete driver or evidence that R3 is finished**. It executes explicitly supplied
synthetic commands without network or provider access. It does not select context,
prepare PostgreSQL snapshots, run Memory retrieval, authorize labels
or mediate paid calls. The earlier shared Docker eval stacks remain unsuitable
for R3 isolation and are not modified by this component.

`runOfflineContainer({ image, argv, files, timeoutMs })` accepts an immutable local
Docker image ID, an absolute executable with argument array, and a map of explicit
relative filenames to UTF-8 contents. There is no host source-directory mount or
inherited environment option. Defaults: 10-second command deadline, maximum
60 seconds, 1 MiB input/output contents, 256 files, 256 MiB memory and 64 PIDs.
The image must supply `/usr/bin/env` and `/usr/local/bin/node`, contain no implicit
volumes, and be reviewed separately. Mutable image tags and automatic pulls are
rejected by the component.

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
fresh reset. Real Rust build images, sufficiently bounded build scratch space and
an explicit source-only export contract still need integration; this small
text-workspace component does not yet run a complete maintenance benchmark.

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

The live test uses trusted Node checks over inert JSON submissions. It proves
the handoff, hidden-feedback exclusion, fake-grader exclusion, positive/negative
controls, deleted-file carryover and fresh sequence reset. It does not yet grade
untrusted compiled Rust programs or claim resistance to attacks on such graders.
Reviewed Rust toolchain images, source export/build scratch limits, existing suite
runner integration and artifact mapping remain unfinished. No new representative
benchmark labels or paid execution are enabled.

Run all component tests with the same explicitly required image configuration:

```bash
MEMORY_R3_REQUIRE_CONTAINER=1 \
MEMORY_R3_TEST_IMAGE=sha256:ae7d65bc9a8e0747b9d1a2e8aa8da051e1bb65dda859064657307c355e3dbdc7 \
node --test evals/r3/isolation/*.test.mjs
```
