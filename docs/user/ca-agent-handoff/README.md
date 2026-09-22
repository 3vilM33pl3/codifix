# One handoff for the CA agent

Give the CA agent this instruction:

> Read CA-AGENT.md and the linked Notion coordination page. Own the verification,
> installation and migration workflow, communicate with the Monolith agent on
> that page, and collect routine results yourself. Prepare one concrete approval
> request for live changes; do not switch the old writer without the separate
> cutover approval. Do not treat missing inputs as permission to bypass a gate.

The [Notion page](https://app.notion.com/p/3e38990715f48117b9e8d8f3a95637d1)
contains the public support download and the handoff dispatch. This is an agent
runbook, not another sequence of commands for Olivier to type.

## What's in the delivered directory

- `CA-AGENT.md`: the complete phased job, known missing inputs and recovery.
- `NOTION-COMMUNICATION.md`: shared page and message/acknowledgement rules.
- `preflight.py`: one bounded, offline verification command; no DB access,
  installations, service changes or model calls. It makes a fresh private copy
  of the selected signed artifacts and a sanitized receipt.
- `SHA256SUMS`: checksums of the **support files**, excluding `resources/release`.
- `references/`: pinned copies of the relevant repository guides/templates.
- `resources/release/`: full unchanged signed release, both architectures,
  source, licenses, tools and its own independent `SHA256SUMS` (full package only).

The small **support ZIP** on Notion omits only the large signed release already
on the CA's USB. To use it, the CA agent verifies the ZIP digest in the dispatch,
unpacks into a **new** local private directory, then copies the existing
`memory-release-2.1.0-rc.1` USB directory to `resources/release` there. Preserve the
original USB files. Alternatively, use `preflight.py --release-dir` pointing to
that unchanged USB directory. No need to copy the release back from Monolith.

Verify the support inventory before executing `preflight.py`; independently
confirm the release signer's fingerprint. The helper checks all 55 original
checksum entries and verifies the signature/stages the host's selected artifacts.
It does not authenticate itself: trust its dispatch digest through the established
operator/Notion channel first. Run as an ordinary local operator with Python
3.11+, GPG and enough private local staging space (allow 300 MiB).

An AMD64 preflight on Monolith is not a native ARM64 CA runtime test. The
helper's schema-60 argument is a release-test assertion, not a DB observation.
It reports those limitations explicitly. No OS upgrade is authorized to satisfy
its minimum tested glibc 2.36 check.

## What this package deliberately does not contain

No credentials, CA keys, private memory content, full source database dump,
all-area archive, authentication state, live jobs, enrollment or supervisor grant.
It is public software plus operational instructions, for internal transfer.
License notices remain beside the immutable software. It does not establish
public redistribution permission for commercial release material.

The Trust dataset is a **separate protected deliverable**, not silently embedded
in this download. The current importer consumes all-area input, so a reviewed,
tested minimized transfer/import method remains a prerequisite. The live source
also needs a fresh delta review; the old staging counts are not today's data.
CA-local/offline data coverage and native checks remain open. The agents should
coordinate those requirements directly instead of asking Olivier to relay logs.

## Rebuild on Monolith (maintainer only)

Use an already verified signed handoff and an existing output parent. The builder
creates a new directory, verifies the pinned release, copies only an explicit
allowlist of support material, rechecks the copied release and writes a support
ZIP. No input is modified and no secret/config/data directories are read.

```sh
/usr/bin/python3 -I -S scripts/areas/ca-agent/package.py \
  --release-dir /data/tmp/memory-signed-release-20260922.0Ov3K6/handoff \
  --output-parent /data/tmp
```

The support archive is not release-signed. Its SHA-256, the release identity and
verification limitations must be included in the dispatch and checked on receipt.

```sh
/usr/bin/python3 -I -S scripts/areas/ca-agent/test_preflight.py
```
