# CA-agent handoff and Notion coordination — 22 September 2026

Olivier requested one Markdown runbook/directory so the CA's local agent can own
the entire job, and a Notion page for communication with the Monolith agent.
The dev Memory task checkpoint succeeded before edits:
`883c628d-203b-40e6-8945-de649c7f7805`, task memory
`2319488a-17f0-4d29-830c-21ec0e0d6604`.

## Delivered preparation

- [Source runbook](../../user/ca-agent-handoff/CA-AGENT.md), package README and
  communication contract; explicit grouped host-change and later cutover gates.
- [Shared Notion page](https://app.notion.com/p/3e38990715f48117b9e8d8f3a95637d1),
  under Memory Layer, with support ZIP and append-only agent coordination comments.
- Full local directory:
  `/data/tmp/memory-ca-agent-handoff-z1k7ce15/ca-agent-handoff`.
- Small Notion/transfer support ZIP:
  `/data/tmp/memory-ca-agent-handoff-z1k7ce15/ca-agent-support.zip`.
- Private assembly receipt:
  `/data/tmp/memory-ca-agent-handoff-z1k7ce15/assembly-receipt.json`.

The full directory includes an unchanged copy of the already signed public
release (source `b35faf2983d20c69bc086fb9bcb7e95c51bd8f47`). The ZIP includes 17
support files and no release binaries/private data. The CA agent combines it
with the already present USB release, avoiding another physical transfer.
No existing handoff or USB file was overwritten.

| Item | SHA-256 |
| --- | --- |
| Support ZIP, 76,476 bytes | `d0fc7c1741d74bbe867ee5062c1d60ec56e802d9c1794b169a18bd3e48b9624d` |
| Support inventory | `80de03f9e457341cf14f207fe2d1fe12d06d804304d925242468e5c7609d736a` |
| Agent runbook | `9c72468059425bfefd576c54837daed69ab3da2441d3366a9ddcc1413a06d962` |
| Preflight helper | `e2feb71cb677e180ed3c747d49a77db9fafb89dfca8b608f057ec2d90e6f2e3b` |
| Original release inventory | `de6339d324bff0a2abd9ff4f1aa697692b7e10aed69d2417baad21b7662517fb` |

The ZIP/runbook are checksum-bound support, **not** new release-signed artifacts.
The signing-subkey trust anchor remains independent. An agent-authored comment
is never passed off as Olivier's approval, even though both connectors may show
his Notion account. The page does not establish automatic wakeups or a running
agent messaging service. Native CA receipt is pending until acknowledged there.

## Verification

The new stdlib helper verifies the fixed checksum inventory, rejects changed or
extra files and links, copies/re-hashes the fixed verifier into private scratch,
and invokes it with isolated Python and a clean environment. Only public files
and private new evidence/staging files are accessed. It does not query a database,
execute the Memory artifact, install a key or change services. Schema 60 is an
explicit offline release-test assertion, never an observed database state.

- Eight unit tests: native architecture/libc classification, paths/links/hashes,
  changed inventory, complete inventory/extra-file rejection and duplicates.
- All 16 support checksums verified after assembly; the support inventory itself
  is the seventeenth support file.
- Real signed-release verification: all 55 original checksum entries, signing
  subkey and selected AMD64 artifact staging passed on Monolith and in a pinned,
  network-disabled Debian 12 container (glibc 2.36, Python 3.11.2, GPG 2.2.40).
- Container image:
  `sha256:ed8f07b19272db7d8759355733e5564a2745a4059f44b7065b130abc97668fac`;
  read-only root/inputs, no capabilities, non-root, no service/DB sockets, no pull.
- Container receipt:
  `/data/tmp/memory-ca-agent-bookworm.TJRnKquS/memory-ca-preflight-8s0mr90c/preflight.json`,
  SHA-256 `c918f72ab54c7739da5e3ec83c02a8785172d4615c0bdf0d68c1d1795ae81959`.
- Source whitespace checks passed. No application behavior changes; full Rust,
  Web or database suites are not claimed for this documentation/helper slice.

The earlier signed-release verification contains both-architecture signature
tests and eight rejection cases; those are retained unchanged, not rerun native
CA evidence. The new run did not execute ARM64 code or prove a CA sandbox.

## Remaining work is explicit

The signed USB contains software, **not a migration dataset**. The current area
importer consumes the complete protected all-area archive. No proven standalone
Trust-only input contract was found; shipping every area's raw data to make that
command work is prohibited. Prepare/test a minimized transfer/import path on
Monolith, perform the current-source dependency-aware delta review, and establish
CA-only/offline data coverage before data migration. Historical Trust counts
(19 histories, 22 versions) are not current-source or CA-import evidence.

After local inspection, the CA agent prepares one concrete host-change request
covering an independent CA-local database, credentials, dependencies, root-owned
installer paths and inactive deployment. The final writer cutover is separately
approved after backup/restore, native compatibility and denial checks. Pi,
federation enrollment, paid calls and R3 remain separate. No CA services, shared
source writers, original data, credentials, DNS or signing material changed here.

## Connector-readable follow-up — 22 September 2026

The CA agent acknowledged the handoff in comment `ca-monolith-20260922-001`:
Notion read/comment access works; it reports native AArch64, glibc 2.36, Python
3.11.2, GPG 2.2.40 and all 55 USB checksums passing. These are CA-agent reports,
not independently observed execution from Monolith. No signature verification,
Memory execution or live installation was reported. Its connector cannot download
the binary support ZIP, so Olivier approved publishing the instructions directly.

Dev task checkpoint `ace06023-24e5-43be-bc6d-9c87d8d592c0`, task memory
`075c760a-a594-454a-8c2d-2c4ee9566a74`, preceded publication of:

- [CA agent runbook — readable without ZIP](https://app.notion.com/p/3e38990715f481d99b06dfbd7f3d512b):
  complete runbook and package README as native page text, with table/link
  presentation adapted for Notion. The original Markdown file/hash is unchanged;
  it is not the hash of Notion's rendered representation.
- [CA preflight helper and release trust references](https://app.notion.com/p/3e38990715f481639a5edb81b5ed0f42):
  literal existing helper source, expected helper/frozen-verifier digests,
  signing-subkey identity and the prior operator-authorized signing-only task
  provenance. A missing independent operator trust reference remains a gate;
  neither the USB public key nor an agent's comment authorizes new trust.

Both are children of the existing coordination page. Fetch/readback found all
15 runbook/README section headings and the final sections; the helper code text
matched the local original exactly (8,820 characters excluding its final newline).
The source SHA-256 remains
`e2feb71cb677e180ed3c747d49a77db9fafb89dfca8b608f057ec2d90e6f2e3b`.
Parent readback verified both links, acknowledgement status and preservation of
the optional ZIP. Reply `9a920f38-edac-4806-a320-44f26a621155` was posted in the
CA agent's discussion and verified present. The next requested receipt is that
the CA agent can read the text and complete scoped native verification.

The preserved local package/ZIP/release were not rebuilt or changed. No new code,
credentials, trust keys, service changes, private-data transfers or cutover were
performed. New source/application tests are unnecessary for this publication-only
follow-up; exact source hashes, Notion readback and repository whitespace were
checked. Delivery of the reply is not acknowledgement or completed CA execution.
