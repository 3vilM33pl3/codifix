# Observation limits

An atomic name replacement concerns what a lookup sees, not whether the reader
has adopted it and not whether the writer's bytes survive a machine crash.
This fixture supplies neither flush evidence nor a power-loss experiment.

The journal establishes a stale open handle and a missed current-state check in
one selected observer. It does not establish a fleet incident count, an operating
system bug, a complete event order during overflow, or the number of requests
affected by stale configuration. Those require additional host inventories,
applied-digest histories and request evidence.

Do not overwrite configuration files as part of diagnosis. W7 is a proposed
consumer contract, not approval to modify live systems. The service should make
invalid-candidate retention visible without leaking the candidate's contents.
