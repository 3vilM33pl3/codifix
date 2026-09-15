# Olive identifier catalog, normalization N2

Catalog entries have positive u32 IDs, an original display label and a normalized
lookup key. This exercise uses an explicit small rule table, not a claim of full
Unicode NFC/NFKC or universal human-name equivalence. Labels contain no whitespace
and at most128 UTF-8 bytes. Keys must be nonempty and at most32 Unicode characters.

Normalize in this order: map fullwidth Latin letters U+FF21..FF3A and FF41..FF5A
to their ASCII letters; lowercase ASCII A..Z and map É to é; replace each e
followed by U+0301 combining acute with é. The resulting key may contain only
ASCII a..z,0..9,underscore,hyphen and é. Reject any other character or unattached
combining mark. For example, both representations of an acute e use one key,
but a plain e remains different. Do not transliterate é into e.

`add ID LABEL` prints `inserted`, `unchanged`, `conflict-id`, `collision:OWNER`
or `invalid`. If ID exists, only an identical original label is unchanged;
another label is conflict-id and cannot overwrite it. Otherwise a normalized
key owned by another ID is a collision; do not allocate a second owner.
Invalid input is rejected before any mutation. Keep the exact display label.

`get LABEL` prints `found:ID:ORIGINAL` or `missing`; invalid labels print `invalid`.
Use the same normalization rules as add, not a different lookup approximation.
`show` prints `entries:` followed by `ID=KEY@ORIGINAL` entries in increasing ID
order, comma-separated. The empty catalog prints `entries:`.

`rename ID LABEL` changes an existing display label and key together, printing
`renamed`; the same display label prints `unchanged`. Missing ID prints `missing`.
An occupied target key prints `collision:OWNER` and preserves the old entry.
An equivalent key owned by the same ID may receive a new display spelling.
`remove ID` prints `removed` or `missing` and releases only that entry's key.
Later add may reuse a released ID/key. No operation silently merges two identities.

Syntax errors, zero/out-of-range IDs, extra tokens and invalid labels print
`invalid` without mutation. This is an in-memory registry, not authentication,
a database migration or a broad Unicode-security implementation. The prototype
lowercased ASCII only and overwrote IDs/duplicate keys; N2 supersedes it.
