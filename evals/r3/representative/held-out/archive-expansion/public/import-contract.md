# Kestrel diagnostic archive import A5

Uploads are untrusted archives, even from enrolled support partners. Maximum
compressed upload is 1 MiB. Accepted expanded content is limited to 16 MiB per
archive and 8 MiB per entry, with at most 128 entries. All units here are binary:
1 MiB = 1,048,576 bytes. The limits apply to observed output bytes, not merely
declared sizes. Count bytes as the decoder produces them and stop before writing
the first byte over a bound.

The expansion-ratio bound is 64 times the complete compressed upload size; it
is an additional total-output limit, not a replacement for per-entry limits.
Archive metadata can be wrong. A valid checksum or partner identity does not
prove bounded expansion, safe paths or harmless content.

Entry names must remain beneath private staging after component validation;
reject parent traversal, absolute names and links. Nothing becomes retrievable
until every entry is checked and the archive is explicitly activated. Rejection
removes only its owned staging work and records the reason without entry bytes
in operational logs. Cleanup uncertainty remains visible for reconciliation.
