# Rowan miniature route table

This fictional in-memory router is not an HTTP server. Commands are one line,
with one output line each: `add ID METHOD PATTERN TARGET`, `remove ID`,
`route METHOD PATH`, `revision`. IDs are positive u32. Methods are GET, POST, HEAD.
TARGET is 1–12 lowercase letters. At most 32 routes exist. Invalid syntax returns
`invalid` and does not change state. Unknown removal or no matching route is
`missing`. A new successful mutation returns `ok` and increments revision once.

Paths start with `/`. `/` is empty path; other paths have nonempty slash-separated
segments containing lowercase ASCII letters, digits, underscore or hyphen. At
most eight segments are allowed, each literal at most 32 characters. No
trailing/double slash, percent encoding, query, fragment, dot or dot-dot is allowed.
Patterns use literal segments, `:name` for exactly one segment, and a final
`*name` for zero or more remaining segments. Capture names are unique lowercase
letters, 1–12 characters. Captured tail segments join with `/`; an empty tail is
an empty capture value. Matching never normalizes unsupported input silently.

## Stage 1 — deterministic specificity

Ignore insertion order and route ID when choosing among matching patterns.
Compare segment kinds lexicographically: literal outranks parameter, parameter
outranks rest. An exact end outranks a rest that matches zero segments. Thus
`/a/b` outranks `/a/:x`, and `/a/:x` outranks `/:y/b` on `/a/b`; `/a` outranks
`/a/*tail` on `/a`. Return captures sorted by name, with no stale captures from
other candidates. Output is `target:TARGET via:METHOD params:k=v,k=v`, or `params:-`
when there are no captures. Keep target and method from the chosen route.

## Stage 2 — method-aware HEAD fallback

Select only the requested method. For HEAD only, first look for matching explicit
HEAD routes; if none match the path, choose among GET matches using the same
specificity. An explicit HEAD match beats a more specific GET route. Other
methods never fall back. `via` records the actual selected route method.

## Stage 3 — atomic table edits

Normalize patterns for conflict detection by retaining literals and segment
kinds but ignoring capture names. Two different IDs with the same method and
normalized pattern conflict (`conflict`). Different methods may share a pattern.
Replacing an ID validates the new entry against all other IDs before commit;
failure retains its old route and revision. An exact repeat of all fields on
the same ID is `unchanged` without a revision increment. Adding a new ID at
capacity is `full`; replacement still works. Removal increments revision only
when an existing route is removed. IDs may be reused after deliberate removal.

Public parser/pattern helpers already define the grammar. Fix selection and table
semantics without adding dependencies, actual HTTP, auth or network behavior.
