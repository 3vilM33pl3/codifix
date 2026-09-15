# Northbank event importer

Fictional dependency-free Rust maintenance repository. `cargo run` reads commands
on stdin. No network, database server or customer records are involved.

Commands:

- `apply SEQUENCE OPERATION_ID DELTA FAULT` submits a signed-authority delivery.
  IDs are ASCII alphanumeric; sequence starts at 1; delta is a signed integer.
  FAULT is `none`, `before` (failure just before commit), or `after` (commit
  succeeds but acknowledgement is lost). Transport authenticity is assumed.
- `restart` simulates replacing the worker while retaining its durable store.
- `show` emits `cursor=N total=N accepted=N pending=N` from durable state.

`apply` emits `applied`, `duplicate`, `queued`, `rejected`, `failed` or `uncertain`.
`restart` emits `restarted`. Invalid command syntax emits `invalid` without
changing state. Do not change this protocol or add dependencies.

Modules separate parsing (`input.rs`), durable state (`store.rs`), the accepted
cursor (`cursor.rs`) and replay (`replay.rs`). The initial implementation has
known recovery bugs. Ordinary ordered delivery works; run the public smoke input
from `smoke.txt` to check the current executable protocol.
