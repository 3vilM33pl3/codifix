# Coherent offline release preparation — 2026-09-22

Direct continuation checkpoint: task `4f34448a-5750-4075-a07c-839e68ad54ea`,
memory `1746ffa3-7278-49ef-9853-f9358ea00d63`, starting at `99684ba4`.

Prepare a single-source AMD64/ARM64 internal Memory release containing the new
inactive console tools. Reuse the captured Bookworm image and public dependency
caches, but preserve all previous artifacts and verify every new input/output.
Build and test offline. Assemble exact binary/Web archives, Debian packages,
source and provenance for **operator signing review**, without selecting or using
a real signing key. Do not sign the candidate with test credentials. Pi is not
rebuilt, admitted or redistributed by this slice; existing pins remain intact.

No CA installation, credentials, service restart, database provisioning/import,
source-writer cutover, provider calls or R3 runs. The approved topology remains a
Trust database **local to the CA**, not a remote Monolith database; the previous
console guide's remote-DB discussion described an unapproved alternative.

Required evidence: both packages from one clean commit, all packaged executable
and Web digests, source verification, dependency/ABI checks, isolated native AMD64
and explicitly emulated ARM64 smoke checks, offline schema-plan agreement,
deterministic archive/manifest validation and fail-closed unsigned verification.
Full application/data recovery and native CA sandbox checks remain independent.

Status: in progress. This document must record actual outputs before the release
can be presented for signing. No publication, signature or installation authority
is implied by successful compilation.
