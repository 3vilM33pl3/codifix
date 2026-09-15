# Build/cache contract

- `node NAME VERSION` creates or updates a source version without discarding its
  existing edges. Versions are positive u64 values. Names contain ASCII letters,
  digits or underscores and are at most 64 bytes. Staging prints `staged`.
- `edge CHILD PARENT` declares that CHILD depends on PARENT; `drop CHILD PARENT`
  removes that edge. Both nodes must exist. Repeating an edge/drop is harmless.
  Missing nodes print `invalid` and do not change the graph.
- `build TARGET ok|fail` traverses the target's full dependency closure. Visit
  dependencies in sorted order, depth first, and build each node once after its
  prerequisites. A reachable cycle or absent target prints `invalid-graph`
  without changing the cache. Unrelated cycles do not block a valid target.
- Reuse an artifact only when its source version and complete direct-dependency
  generation map match the current candidate. Adding/removing an edge changes
  that map. A changed dependency propagates invalidation downstream, without
  rebuilding unrelated nodes or unchanged prerequisites.
- New artifacts receive increasing generation numbers, starting at 1. Successful
  replacement prints `built:` followed by rebuilt names in build order. If all
  artifacts are reusable, print `cached` (even with a `fail` injection).
- A `fail` injection on nonempty work prints `failed`, preserving every prior
  artifact and the generation counter. Retrying must perform the same work.
- `show` prints `cache:` followed by sorted `NAME@generation` entries. Malformed
  commands print `invalid`. A staged source/edge change never itself edits cache.

An artifact-generation counter is a deterministic build test double. This does
not implement a real compiler, filesystem cache, scheduler or distributed build.
