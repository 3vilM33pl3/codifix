# Configuration contract

The service starts at revision 0 with port 8080, timeout_ms 1000, log_level info
and banner ready. Keys and values are whitespace-free tokens.

- `base KEY VALUE` stages a base value. `set KEY VALUE` stages an overlay value.
  Explicit overlay values take precedence over the base.
- `unset KEY` stages an explicit deletion. It masks the base even if the base
  subsequently changes. `clear KEY` removes that overlay instruction and restores
  ordinary base fallback. Staging returns `staged` and does not alter live state.
- `commit EXPECTED ok|fail` computes the candidate configuration, checks the
  expected live revision and schema, then attempts the deterministic reload.
  Successful reload returns `applied` and advances revision once. Draft state
  remains available for subsequent changes. There is no implicit draft reset.
- A mismatched revision returns `conflict`. An invalid candidate returns
  `invalid-config`. The `fail` injection returns `reload-failed` only for an
  otherwise valid candidate. All three failures preserve live values and revision.
  A failed reload can be retried with the unchanged expected revision.
- `get KEY` reads live state, returning `missing` for an absent key. `show` prints
  `revision=N`, then sorted `key=value` lines. Malformed commands return `invalid`
  without changing state.

The mandatory fields are port (1..65535), timeout_ms (1..60000) and log_level
(off, info or debug). Banner is optional and at most 80 bytes. Unknown keys are
invalid. The supplied schema module encodes these rules.

This is an in-process reload test double. It does not model partial side effects
of an actual external reload and does not claim production rollback guarantees.
