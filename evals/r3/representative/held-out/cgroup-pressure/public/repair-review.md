# M13 repair proposal — pending

Rename heap-only telemetry and add group charge/headroom measurements using the
same disjoint category contract. Budget admitted work by bounded bytes and total
working set, not only heap size or batch count. Account for native buffers,
resident mapping and supervisor overhead before starting more work. Release
owned buffers on completion/failure and test oversized individual batches.

Validate realistic high-water samples and pressure behavior in a disposable
environment before choosing a new limit or concurrency cap. Increasing a group
limit is a separate capacity decision, not a demonstrated cure for an unbounded
admission path. Do not delete data files or drop source records to make a graph
look smaller. No production limit change or successful rollout is approved.
