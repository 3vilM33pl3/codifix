# Synchronized knowledge (Offline)

## Purpose

Read-only local inspection of explicitly configured subscriptions. This is not
the ordinary Query tab, remote search, current authority, or model forwarding.

## Layout

The subscription selector shows exact project/owner identity and committed cursor.
The scrollable results show plain reviewed text, original authors, source/shared
versions and evidence digests. Unknown or old observations are labelled explicitly.

## Controls

- `r`: refresh inventory; old excerpts and pending searches are discarded.
- `j/k` or arrows: select by subscription UUID, not project slug.
- `/`: enter a local search question (at most 4096 UTF-8 bytes).
- `Enter`: finish entry and search the initialized exact subscription (8 hits).
- `Esc`: end entry and clear old search results.
- `d/u` or PageDown/PageUp: scroll excerpts and citation/evidence metadata.

## Workflows

Refresh, select an initialized subscription, press `/`, enter a question and
press Enter. This searches the local approved copy without a model or network
fallback. Changing subscriptions discards and cancels old results.

Shared text and locators are untrusted plain text, never executed. Last observed
sync and cursor describe a verified local copy, not current origin access or
freshness. Previously delivered bytes cannot be recalled by revocation.

The configured loopback local owner must authenticate. Unsupported builds,
disabled cache policy and denied reads are errors, not a healthy empty cache.
