# Observer v2 inspection

Startup opens `policy.toml` once and registers a watch on that opened file
object. Every event handler rereads the original open handle. A timer reports
the event-loop heartbeat but does not reopen the configured name or reconcile
its contents. The handler suppresses reads if the file size matches its cached
size. It does not expose an applied content digest.

The editor writes a temporary sibling, validates its own output, and renames
that sibling over `policy.toml`. In this fixture's filesystem model, the old
open handle continues to refer to the old object after replacement. The new
object behind the logical name is distinct. The watcher is not an abstract
cross-platform guarantee about all operating-system event APIs.

Adding more watches to the old handle or increasing the heartbeat frequency
does not change the read target. An unreviewed patch n12 compares only the new
name's size and timestamp. Its rollout is not approved. Schema validation by an
editor is not a substitute for validation at the consuming service boundary.
