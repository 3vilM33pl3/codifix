# Synchronized knowledge (Offline)

Read-only local inspection of explicitly configured subscriptions. This is not
the ordinary Query tab, remote search, current authority, or model forwarding.

- `r`: refresh inventory; old excerpts and pending searches are discarded.
- `j/k` or arrows: select by subscription UUID, not project slug.
- `/`: enter a local search question (at most 4096 UTF-8 bytes).
- `Enter`: finish entry and search the initialized exact subscription (8 hits).
- `Esc`: end entry and clear old search results.
- `d/u` or PageDown/PageUp: scroll excerpts and citation/evidence metadata.

Shared text and locators are untrusted plain text, never executed. Last observed
sync and cursor describe a verified local copy, not current origin access or
freshness. Previously delivered bytes cannot be recalled by revocation.

The configured loopback local owner must authenticate. Unsupported builds,
disabled cache policy and denied reads are errors, not a healthy empty cache.
