# Hazel structured disclosure filter

This fictional in-memory exporter stores structured records as dotted field paths
with UTF-8 values. It never executes their contents. No real credentials, models,
HTTP or file paths are used; strings such as secret/path are synthetic controls.

Commands emit one line each:

- `put ID KIND key=HEX;key=HEX` replaces raw fields (`-` for empty). ID is a
  positive u32, KIND is 1–12 lowercase letters; unknown kinds can be stored but
  not exported. Duplicate keys are invalid. Keys are at most four dot-separated
  lowercase-letter segments, each 1–12 characters. At most 16 fields, each at
  most 128 decoded UTF-8 bytes. `-` represents an empty value after `=`.
- `grant ID human|model`, `revoke ID human|model`, `drop ID`: `ok`, `unchanged`
  for an ineffective grant/revoke, or `missing` for an absent ID.
- `export ID DEST`: sorted allowed `key=lowercasehex` pairs separated by `;`.
  Missing record is `missing`, absent/unknown destination grant is `denied`,
  invalid eligible schema is `schema`. No partial fields accompany an error.
- `generation`: number of successful puts/drops/effective grant changes; begins 0.

Malformed commands are `invalid` without changes. Export destination is any
lowercase name so unknown values can fail closed as `denied`. Grant/revoke accept
only the two configured destinations. Put returns `ok`, even for stored schema
that later fails export validation. Every replacement clears existing grants;
source changes require renewed disclosure authorization. Drop removes grants.

## Stage 1 — per-schema allowlists

Incident human exports allow only title, severity, author, evidence.label and
evidence.digest. Metric human exports allow only name, value and unit. Private
body, auth.token, debug, paths and all unknown fields are never exported, even
when they contain an allowed word. Match exact field paths, not key substrings.
Keep allowed strings intact as data and render canonical lowercase hex.

## Stage 2 — validated structural shape

Only incident and metric schemas may export. Incident requires nonempty title
of at most 64 bytes and severity exactly one ASCII digit 1–5. Optional author is
nonempty if present. evidence.label and evidence.digest must appear together;
label is nonempty, digest is exactly eight lowercase ASCII hex characters.
Metric requires a lowercase-letter name of 1–12 characters and value in canonical
i64 decimal form (no plus, leading zeros or negative zero). Optional unit is a
lowercase-letter name of 1–12 characters. Unknown fields stay private.

Any simultaneous parent/descendant keys (for example evidence and evidence.label)
are ambiguous and reject the entire export, even if the parent is private. No
prefix trick can reinterpret a scalar as a nested object. Rejection leaves the
raw record, grants and generation unchanged.

## Stage 3 — exact destination consent

Check record existence and current destination grant before schema validation.
Human and model grants are independent; unknown destinations never inherit them.
Model incident output is narrower: title, severity and evidence.digest only;
model metric output is name, value, unit. Model grants do not authorize author or
evidence label. Validate the complete required schema before filtering destination
fields. Revoke immediately denies subsequent exports without deleting the record.
Source replacement or drop/reinsert cannot reuse an old grant. Export is read-only
and never changes generation. Do not add dependencies or blanket-deny valid work.
