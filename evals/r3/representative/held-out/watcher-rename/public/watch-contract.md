# Hazel configuration observer — W7 contract

This fictional service observes the logical configuration name `policy.toml`
inside an operator-selected directory. Editors may replace that name atomically
with a newly written file. Correctness concerns the bytes currently reachable
through the configured name, not the lifetime of the object opened at startup.

W7 observes directory changes as hints and periodically reconciles the selected
name. Reconciliation opens the current candidate, reads it completely, validates
its schema, and installs a complete valid generation. Invalid, missing or
unreadable candidates retain the last valid generation and expose a diagnostic.
Never install a partially parsed configuration. Compare a content digest, not
only size or modification time. Repeated hints for the same valid bytes do not
create new applied generations.

A lost-event/overflow hint requests a full reconciliation, not a guessed list
of missed updates. Observe only the selected name, not every sibling file. This
contract does not specify recursive filesystem watching or prove crash-durable
writer storage. It requires an application-level applied digest and generation
to distinguish observer liveness from successful configuration adoption.
