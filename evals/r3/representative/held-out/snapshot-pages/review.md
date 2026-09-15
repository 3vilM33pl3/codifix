# Held-out coding family: snapshot pages — DRAFT

Five modules and two ordinary public tests model immutable reads over mutable
in-memory incident rows. Cursor strings model bindings, not cryptographic tokens
or deployed project authorization. Related knowledge stays in the held-out split.

1. Three probes test tied severity, project-before-limit selection and mixed sort
   direction. Row IDs and continuation markers matter, not merely page length.
2. Three probes test insertion/deletion, project/severity changes and initially
   empty snapshots. Existing pages stay stable while new snapshots see changes.
3. Four probes test project/snapshot/tuple binding, malformed/empty cursors and
   expiry without ID reuse or silent restart. Valid continuation remains usable.

Ten private probes and the reference snapshot module never enter a model's
workspace. Reference and cumulative tests must pass; starter/no-op controls must
fail each stage. Accept any contract-compliant solution. Labels remain draft;
fixture verification is not held-out model calibration or paid execution.
