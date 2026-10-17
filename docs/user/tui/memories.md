# Memories Tab

Use **Memories** to browse the canonical project memory rather than relying on a summary alone.

![Memory list and detail panes in the TUI](https://codifix.dev/images/tui/memories-tab.png)

The left pane lists memories; the detail pane shows canonical text, type, confidence, tags, provenance, related memories, validation state, and any proposed replacement. Related memories are navigation hints computed from overlap, tags, provenance, and relationships, not a separate source of truth.

## Synchronized owner knowledge

On a curated-sync laptop, this includes downloaded owner knowledge as well as
local work. Downloaded rows are labelled **Synced/read-only**; selecting one
shows its content, original author, owner, version and digest. `Shift+H` reads
retained owner history. Delete and validation actions cannot modify these copies;
use a correction proposal for owner review. Accepted local contributions are
hidden once their authoritative owner copy has arrived, without deleting local
history. Pending or conflicting local work stays visible.

The laptop list refreshes locally every 30 seconds (`r` refreshes immediately).
It uses a protected, revision-bound browsing view rather than the local-only
stream. This does not change exports, plan selection, evaluation, or the owner
database. An unconfigured or publication-only project retains its local list.

## Controls

- `j` / `k` select a memory.
- `/` edits the text filter; `g` edits the tag filter.
- `s` cycles status filters and `t` cycles memory types.
- `x` clears filters.
- `PgUp` / `PgDn` and `Home` scroll detail.
- `c` runs curation; `i` reindexes chunks; `e` re-embeds the active space.
- `v` previews evidence-backed validation for the selected memory. `y` applies the visible preview as a new version; `n` dismisses it.
- `a` archives low-value memories and `Shift+D` deletes the selected memory.

Use this tab before adding a duplicate, when inspecting source evidence, or when reviewing a fact that may be stale.

See also: [Remember](https://codifix.dev/docs/reference/cli/capture-curation) and [Embeddings](https://codifix.dev/docs/tui/embeddings).
