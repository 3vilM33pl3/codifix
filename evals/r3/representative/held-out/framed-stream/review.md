# Held-out coding family: framed stream — DRAFT

Five Rust modules model a byte-stream receiver through a deterministic command
shell. Two ordinary public tests pass in the starter. Hidden probes never enter
the agent workspace. This is not a live socket, authentication or durability test.

1. Three probes cover split headers/payloads, coalesced/empty frames and a UTF-8
   character split across chunks. The initial receiver drops trailing bytes and
   assumes one frame per feed; a no-op must fail positive delivery checks.
2. Three probes require early length limits, whole-feed rejection on bad UTF-8,
   poisoned state and non-mutating command errors. Previously delivered counts
   persist, but frames from the failing call are not partially published.
3. Four probes cover partial-header/payload EOF, clean close, rejected later
   input, explicit reset, and the distinction between an empty frame and no frame.
   Earlier probes remain active throughout the cumulative three-step sequence.

The reference decoder is private and should pass all ten probes; starter and
no-op implementations must fail each stage. Accept any implementation satisfying
the public contract, not particular source strings. Keep the complete family in
held-out; do not use it to calibrate development prompts or tune quality gates.
Labels require later approval and no model/provider call is authorized here.
