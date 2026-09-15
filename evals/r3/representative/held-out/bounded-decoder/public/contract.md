# Larch bounded run decoder

This in-memory utility decodes a fictional run-length byte stream. Each run is
two bytes: a nonzero count (1–255) and one byte to repeat. Feed boundaries may
split the pair. The count/value bytes are not text and every value byte is valid.
No files, compression libraries or external payloads are used.

Commands produce one line each:

- `begin LIMIT`: start/replace a private staging stream with output limit 0–64,
  returning `ready`. This deliberately discards any uncommitted stream/error.
- `feed HEX`: append 0–32 encoded bytes (`-` for empty), returning `queued` on
  accepted input. Hex must be even-length with ASCII hex digits.
- `finish`: commit a complete active stream, returning `committed:GENERATION`.
- `show`: return `GENERATION:HEX`, lowercase hex or `-` for empty. Starts `0:-`.
- `status`: `idle`, `failed`, or `active in:N out:N pending:yes|no`.

Malformed commands/hex are `invalid` without changing any state. Inactive or
already-failed feed/finish is `inactive`. Begin never alters the committed result.

## Stage 1 — streaming pairs

Retain an incomplete count until its value arrives. Never silently drop an odd
suffix or interpret a value as another count. Count zero fails the stream with
`malformed`; EOF with a pending count fails with `truncated`. Empty stream is a
valid empty result. Any stream failure sets `failed`; begin is required to restart.

## Stage 2 — pre-expansion limits

At most 32 encoded bytes may be accepted over the entire stream, not per feed.
Before expanding a run, check the actual accumulated output plus count against
the chosen output limit, even for repeated runs of the same byte. Fail with
`limit` before allocation/expansion if either cap would be exceeded. Exactly the
limit is allowed; advertised/compressed size is not an expanded-size guarantee.
Zero output limit allows an empty stream only.

## Stage 3 — atomic publication and recovery

All decoded bytes stay private until successful finish. Failed/truncated/limited
streams never change the prior committed bytes or generation. A failed feed
cannot expose partially decoded bytes or be continued; begin clears staging and
error, not history. Successful finish increments generation once and becomes
idle; repeated finish cannot commit again. Mid-stream show always reads the
previous committed generation. This is not durable disk recovery or a guarantee
against every real decompression attack. Do not add dependencies.
