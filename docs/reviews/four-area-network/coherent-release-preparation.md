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

Pre-build review found an integration defect: the legacy package owns
`/etc/memory-layer` and `/var/lib/memory-layer` as its service user, while the
privileged console correctly requires trusted root-owned ancestors. New named
instances now need independent `/etc/memory-layer-instances` and
`/var/{lib,cache}/memory-layer-instances`, `/run/memory-layer-instances`, plus the
root-private `/var/lib/memory-layer-installations` journal. Keep `/opt` release
paths unchanged. Do not chown legacy directories or adopt existing older-layout
instances. Add a real-root container regression using service-owned legacy roots
before accepting this corrected candidate.

The correction passes 26 signed console/unit tests and 10 real-root CLI tests.
The latter uses service-owned uid/gid 65534, mode-0750 legacy directory fixtures,
no capabilities and only read-traversal supplementary group access. All legacy
bytes, modes, owners and inode identities remain unchanged. Five older-layout
instance/journal locations refuse install/inspection/recovery without adoption.
This is fixture evidence, not execution of package hooks against the real host.
Evidence: `/data/tmp/memory-console-cli-f0i1j1x9/`; 35 exact fake-systemctl read-only
queries, no activation markers. Corrected console SHA-256:
`3e5ed1ad489951fd91fd0047d2c4441d33fe233df9a6f3ddd90ad8e79c526675`.
Reviewed unit SHA-256:
`418e629337592d207f11115226395015c11aa01223841692771b9e12f4db1dca`.
The 20 prepared-instance tests, three launcher tests, Debian lifecycle fixtures,
seven build-command tests, formatting and boundaries also pass.

Status: in progress. This document must record actual outputs before the release
can be presented for signing. No publication, signature or installation authority
is implied by successful compilation.
