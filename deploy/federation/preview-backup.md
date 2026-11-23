# Coordinated preview backup and interrupted shutdown recovery

`preview-backup.mjs` operates only on the exact container IDs and private paths
in a reviewed configuration. It does not discover writers or back up the shared
source database. Keep source snapshots and their verification separate.

The configuration pins four preview writers, three databases, approved archive
paths, the NAS mount and encryption recipient. Its SHA-256 review binds each
operation. Configuration files/directories must be owned by the operator and
private (0600/0700); do not put credentials in command arguments.

If refresh-only maintenance is active, include the optional exact field
`"maintenance_worker": "codifix-pds-refresh-only.service"`. This changes the
review hash and explicitly allows stopping that unit before stopping the preview
writers. An active maintainer without this opt-in rejects the backup before any
shutdown. Other units are not accepted. Include the approved maintenance state
and access-bundle paths in the existing private archive allowlist as appropriate.

```sh
node deploy/federation/preview-backup.mjs /private/backup-config.json plan
node deploy/federation/preview-backup.mjs /private/backup-config.json preserve REVIEW_SHA256
node deploy/federation/preview-backup.mjs /private/backup-config.json resume REVIEW_SHA256
```

`preserve` durably records the shutdown intent, stops the paid question worker
and optional maintainer, then stops the pinned preview writers. It checks for
unexpected database writers and unsettled model attempts before dumping data.
`resume` requires both encrypted archives to match the successful backup
receipt, including the exact NAS destination. Neither phase claims a successful
restore; that requires the separate isolated restore verification.

If preservation fails after shutdown, inspect the protected intent and actual
container state. The explicit availability-recovery operation is:

```sh
node deploy/federation/preview-backup.mjs /private/backup-config.json recover-shutdown REVIEW_SHA256
```

It accepts only the same configuration, intent and pinned container identities;
the three databases must still be running. It starts only the four original
writers in reverse shutdown order. It can operate while the NAS is unavailable.
Partial archives and dumps remain untouched. Its separate
`shutdown-recovered.json` explicitly records `backup_verified: false` and
`health_verified: false`; it is not a backup receipt or permission to delete
partial evidence. Exact replay is harmless. Use a new timestamped backup
directory for a fresh preservation attempt.

No recovery path starts the paid worker or refresh maintainer. Verify health,
pending work, session reconciliation and current grants before separately
resuming either under the corresponding operator procedure.

The operation lock is a private regular file using Linux kernel `flock`; process
death releases it without deleting the file or guessing whether a PID is stale.
A legacy directory lock fails closed and requires operator inspection. Never
automatically remove it or run two recovery commands concurrently.
