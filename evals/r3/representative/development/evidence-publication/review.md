# Evidence publication — development labels DRAFT

One five-module repository, three cumulative steps, one independent observation.
The positive reference and buggy/no-op controls are verified in the isolated Rust
runner. Grading compares protocol results and immutable history, not code style.

1. Freeze reviewed bytes; reject review borrowing and review-ID replacement.
2. Preserve original author separately from correction editor and authenticated
   publisher. Reject stale bases and preserve old versions.
3. Recheck current grants; recover acknowledgement loss through exact request
   identity without appending a duplicate or accepting altered retries.

Only public/ and the current step prompt belong in agent context. private/
contains reference implementation and hidden probes. No private grader feedback
may enter later steps. The local store is an in-process test double; no claim of
real network authentication, signatures or disk recovery follows from this test.
The labels remain draft and do not represent Olivier's benchmark approval.
