# Held-out knowledge family: stream framing — DRAFT

Five fictional sources and four questions. Transport boundaries, frame lengths
and UTF-8 payloads are distinct; all bytes are supplied as synthetic hex.

1. a emits é/A; b emits B/empty/C. State must survive reads and loop over frames.
2. c is truncated, d oversize, e invalid UTF-8. f retains prior A but does not
   deliver B from the failing second feed. This whole-feed rule is explicit.
3. A larger read buffer is not a state machine. Test splits, coalescing, bounded
   failures and new-connection reset without silent byte repair.
4. Framing does not establish delivery idempotency, traffic loss or throughput.

Require citations and blinded semantic review; equivalent exact explanations
are acceptable. Private draft labels stay outside answering context. The related
framed-stream coding repository stays wholly in the same held-out split.
