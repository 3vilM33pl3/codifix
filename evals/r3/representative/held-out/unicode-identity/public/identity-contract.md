# Violet catalog identity policy U4

Stable account IDs own grants and audit history. Display labels are editable
metadata, not authority. Lookup keys use this exercise's explicit rule table:
ASCII uppercase becomes lowercase; É becomes é; e followed by U+0301 combining
acute becomes é. Plain e is NOT equivalent to é. The allowed key alphabet is
ASCII lowercase letters,digits,underscore,hyphen and é. Other script characters
and unattached combining marks are rejected, not transliterated automatically.

This small table is a fixture policy, not a full Unicode normalization standard.
Preserve original label bytes for provenance while deriving a separate key.
Normalized-key collision between existing IDs requires explicit reconciliation;
never pick the older/more privileged record silently or merge grants by name.

New writes and lookups must use the same rule revision. A collision prevents
ambiguous name-based access; it does not delete the accounts' ID-bound history.
An authenticated operation by stable ID still follows its own current permissions.
