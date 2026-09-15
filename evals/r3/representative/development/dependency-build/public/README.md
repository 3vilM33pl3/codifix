# Birch dependency build planner

Small fictional Rust application with graph, cache, command and planner modules.
No dependencies or external builds. `cargo test --offline --locked` exercises
the two ordinary public tests; `cargo run --offline` reads commands from stdin.
The operational contract is in `contract.md`.

The current single-target path works. Reported failures concern deep dependency
graphs, stale downstream artifacts and cache changes after a failed build. Keep
the command protocol and earlier behavior while repairing the planner.
