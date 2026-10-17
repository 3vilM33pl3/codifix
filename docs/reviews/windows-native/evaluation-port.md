# Native Windows evaluation port

Status: bounded native process-launch seam implemented and independently
reviewed; **campaign integration and native acceptance remain pending**. Part of
the approved Windows parity contract. Paid R3 remains paused; datasets, labels,
scoring, approval budgets and existing Linux artifact meanings do not change.

## Reuse the harness

Introduce an explicit, manifest-bound execution backend at
`evals/r3/isolation/execution.mjs`. Keep `container.mjs` as the unchanged Linux
backend. `sequence.mjs`, `knowledge.mjs` and `model-agent.mjs` use that seam.
The Windows adapter calls a native Rust helper with bounded structured inputs,
not shell strings or an automatic translation of model-generated POSIX commands.

Model calls already happen in the trusted `createModelAgent` /
`dispatchModel` / `request-mediator` path. Keep that division: coding sandboxes
need neither network capabilities nor a provider broker handle. Hidden expected
outputs remain with the trusted grader. `gradeRustCase` uses a fresh sandbox
containing only submitted source and its explicitly selected probe inputs.

## Separate sandbox profile

Reuse AppContainer token verification, suspended launch, explicit inherited
handles, Job assignment, cancellation and cleanup. **Do not loosen Pi defaults.**
Native evaluation needs a distinct profile:

- Fresh zero-network-capability AppContainer for each invocation.
- Hash-bound, read/execute-only native Node/Rust/linker/runtime tree, not the
  operator's development directory or user home. The Pi flat code bundle cannot
  represent a Rust sysroot/MSVC SDK.
- Read/write/execute workspace for compiled submissions, with a process limit
  suitable for Node, rustc, linker and submitted programs (not Pi's limit of two).
- Explicit minimal environment, bounded output and real-time deadline, private
  scratch, no provider/database credentials or other-condition/hidden-test grants.
- After the whole Job is empty, export only allowlisted regular files using
  protected handle/reparse/hard-link checks. Release pins before removing staging.

Job Objects do not provide the Linux tmpfs disk limit. Record that difference or
use a genuinely bounded scratch volume before claiming an equivalent hard limit.

`mem-platform::windows_evaluation` now supplies the separate launch profile:
recursive private toolchain inventories/digests, retained file identities,
reparse/hard-link/alternate-stream rejection, fresh executable scratch, three
explicit stdio pipes, no ambient credentials or network capabilities, a
16-process/2-GiB Job and a host-enforced deadline. Pi retains its existing
two-process/512-MiB profile. Seven Linux tests and Windows cross-target Clippy
passed; Astra found no additional concrete defect in the bounded seam. These
are not native execution results. The feature-gated
`memory-windows-evaluation-helper` now also supplies hash-bound structured
requests, concurrent bounded I/O, private I/O preparation and allowlisted
post-Job byte exports. The explicit JavaScript `execution.mjs` /
`native-windows.mjs` seam binds a distinct Windows profile instead of reusing a
Docker image identity; it never infers Windows or translates POSIX commands.
Nine fake-helper protocol tests passed, including loader-failure rejection.
Existing sequence/knowledge/Rust-grading consumers remain on their unchanged
Linux paths. Native campaign wiring, protected host journals, disk-limit
treatment, packaged toolchains and real Windows execution remain pending.

## Remaining host adapters

| Source | Required change |
| --- | --- |
| `isolation/guest.mjs`, `rust.mjs` | Native tool paths, whole-tree cancellation, protected output collection |
| `isolation/service-session.mjs` | Native process cancellation and private configuration |
| `campaign-dispatch.mjs`, `offline-driver.mjs`, `approved-driver.mjs` | Protected native IPC; remove Unix socket/chmod/directory-fsync assumptions |
| CLI `eval_campaign.rs` and `preparation.rs` | Explicit pinned Node/script execution rather than shebang launch; Job-owned cancellation |
| `request-mediator.mjs`, `mem-eval` budget/provider results | Protected Windows journal creation, locking and reads |
| `paid-runtime.mjs`, `rehearse.mjs` | Generate a separate zero-spend Windows rehearsal pack |
| `mem-eval` campaign isolation contract | Explicit backend, OS/build, architecture, helper/toolchain hashes, limits and boundary-evidence hash |

Keep native PostgreSQL snapshot preparation through the existing
`mem-test-support/examples/r3_snapshot.rs`. Bind the Windows isolation descriptor
into configuration hashes; reject mixed-profile evidence. Old Linux artifacts
remain readable but cannot masquerade as Windows evidence or carry its approval.

## Offline acceptance

- [ ] Deterministic knowledge and three-step coding controls run natively;
  native compilation succeeds and broken submissions remain failures.
- [ ] Hidden expectations, other conditions/projects, host credentials and
  unauthorized network destinations are inaccessible.
- [ ] Timeout/cancellation reaps descendants; flooding and unsafe exports fail.
- [ ] Only selected files persist between steps; independent cases reset.
- [ ] Snapshot and reinforcement hashes remain unchanged.
- [ ] Interrupted dispatch preserves receipts/uncertainty without duplicate sends.
- [ ] Successful cleanup removes staging; failed cleanup is actionable.
- [ ] Missing/mismatched native toolchain or backend metadata invalidates a run.

This proves harness behavior, not measured product quality or paid R3 completion.

## Observed bounded native execution — 17 October

`NativeEval` operation `a3e52a7a-bc30-42ac-8b73-2e966f507cc6` passed actual
AppContainer knowledge/citation checks, excluded-project/hidden-grader denials,
a fake-model-mediated native command plus final export, and output/deadline
failure controls. No provider calls were made; human rubric judgments remain
unresolved. The Task Scheduler registration proof is separate from evaluation.

Pinned Node starts with explicit preserve-symlink flags rather than drive-root
ACL grants. Its child stdio uses private inherited scratch-file descriptors:
pinned libuv's ordinary non-LOCAL pipe creation cannot run in this AppContainer.
Native Rust grading uses that same explicit-descriptor technique. Its post-run
output cap is not a continuous scratch-disk limit. Operation
`a8cf16d2-5913-4ee7-ac68-48bb6b5a2e80` subsequently passed actual native Rust
positive/wrong-output/broken-source controls with MSVC and the private reviewed
toolchain subset. Three-step continuity and campaign dispatch remain pending;
the acceptance checkboxes above are not closed by these narrower checks. See the
[full execution ledger](parity-20261016.md) for exact operations and limitations.
