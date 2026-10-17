# Codifix TUI documentation captures — 2026-10-17

The current TUI images were captured from real tmux panes and rendered by the
repository's terminal capture script. The isolated `codifix-showcase` project
contains only the embedded synthetic demo corpus. Its disposable PostgreSQL
database and loopback service are separate from Workshop and all live areas.
No model provider, embedding provider, watcher, federation or automation ran.
The query screenshot exercises the keyless lexical fallback successfully.

Overview, query and Skills captures were visually inspected; all captured panes
were checked for stale branding and error states. The Skills filter/source/repair
labels and embedded help now use Codifix. Paths in that screen truthfully retain
the current `/home/olivier/Projects/memory` checkout; refresh it after the final
checkout move rather than editing screenshot pixels. The old infographic is
preserved as historical evidence outside the public website tree.

The demo source also now describes the explicit Workshop helper route. A TUI
capture found the former bundled demo branding and stale dev-first guidance;
both were corrected before these captures. The preview service was stopped
when capture finished. Private demo data and raw ANSI captures are retained.

Validation: four skills-library tests passed. Of 19 CLI skill-related tests,
18 passed and one expected an obsolete generated skill title; that assertion
was corrected and its exact rerun passed. The current CLI build succeeded.
Documentation content (99 files), links, 15 public image references and the
production docs build passed. These captures establish documentation state,
not native installation or live operational acceptance.
