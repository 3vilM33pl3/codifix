# Spruce configuration service

A fictional multi-file Rust application. No dependencies, networking or real
deployment. Work across the parser, schema, merge and live-state boundary.
Keep the public command protocol intact. See `contract.md` for semantics.
`cargo test --offline --locked` runs the two ordinary public tests.

Run `cargo run --offline` and send commands on stdin. The public smoke input
below must still print `8080`, `staged`, `applied`, `info` on separate lines:

```text
get port
base log_level info
commit 0 ok
get log_level
```

The current implementation passes that smoke path but has reported problems with
overrides and failed reloads. A successful build or the public smoke alone is not
complete acceptance.
