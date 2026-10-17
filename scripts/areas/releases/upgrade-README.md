# Upgrade an existing named instance

`upgrade.py` upgrades one existing `codifix@NAME.service`. It is separate
from the inactive first installer and leaves its first-install restrictions
intact. It does not modify singleton installations, containers, other units,
configuration, enrollment, sandbox drop-ins, or signing keys.

The executable and complete Web directory switch together with Linux
`renameat2(RENAME_EXCHANGE)`. The previous tree stays alongside the active tree.
The matching executable digest credential changes while the selected service
is stopped. The existing named service launcher starts the candidate under its
existing sandbox. Normal service startup applies append-only migrations; the
operator checks the observed database head after health succeeds.

## Inputs and prerequisites

- Use root-owned, independently authenticated packaged tooling with
  `python3 -I -S`; the entrypoint validates sibling code before importing it.
- Prepare both previous and candidate releases using `instance.py prepare`.
  Both require valid signatures from the supplied enrolled signing fingerprint.
  The installed binary/Web bytes must exactly match the previous release. The
  candidate must be strictly newer on the same native architecture. Supported
  `3.0.0-rc.N` versions order numerically; stable `3.0.0` follows every supported
  candidate. Equal versions and stable-to-candidate downgrades are rejected.
  Signed versions and existing canonical manifest bytes are never rewritten.
- The signed candidate schema interval must include the current observed head;
  transitions must declare migrations in the release manifest. Startup must
  reach its signed head. No schema downgrade command exists.
- The named instance must use the packaged systemd template, a loopback service
  endpoint and explicit loopback PostgreSQL credentials in its protected TOML.
  Install the PostgreSQL client at `/usr/bin/psql`. Its connection is read-only;
  passwords go through its finite process environment, never argv or output.
- Take the coordinated backup and stop external writers in the deployment
  runbook. The operator stops only this exact named unit. Before switching, it
  rejects other sessions on that database. The external writer fence must remain
  effective throughout the operation: observing no sessions is not a persistent
  database-role fence.
- Supply the reviewed current configuration digest, schema number and a fresh
  32-character lowercase hexadecimal operation ID. Retain that ID for recovery.

## Upgrade

```bash
sudo python3 -I -S /usr/libexec/codifix/releases/upgrade.py upgrade \
  --instance trust --operation "$CODIFIX_UPGRADE_OPERATION" \
  --previous "$CODIFIX_PREVIOUS_PREPARATION" \
  --prepared "$CODIFIX_CANDIDATE_PREPARATION" \
  --trusted-signing-fingerprint "$CODIFIX_RELEASE_SIGNING_FINGERPRINT" \
  --config-sha256 "$CODIFIX_REVIEWED_CONFIGURATION_SHA256" \
  --schema-current "$CODIFIX_OBSERVED_SCHEMA"
```

The private journal is `/var/lib/codifix-upgrades/NAME/OPERATION/`. It holds
reauthenticated release snapshots, the exact configuration and previous digest
backup, configuration/systemd hashes, and a hash-chained durable event sequence.
It contains credentials in the protected configuration backup: never publish
it, copy it to Notion, or include it in an unencrypted support archive. Public
results contain only identifiers, hashes, release version and status.

Only one operator can hold the per-instance lock; an unfinished operation blocks
a second operation. A failure before `plan.json` exists has not stopped or
changed the installed service. Preserve that incomplete preparation for diagnosis
and move it out of the journal directory before starting a new operation.

Existing configuration and loaded unit/drop-in hashes are checked again during
recovery. A changed sandbox/configuration requires inspection, not silently
adopting new inputs. All service actions use the exact named unit with bounded
timeouts, finite environment and no shell. Health requests are bounded loopback
requests without proxies or redirects and must report the candidate version.

## Inspect and recover

```bash
sudo python3 -I -S /usr/libexec/codifix/releases/upgrade.py inspect \
  --instance trust --operation "$CODIFIX_UPGRADE_OPERATION" \
  --trusted-signing-fingerprint "$CODIFIX_RELEASE_SIGNING_FINGERPRINT"
sudo python3 -I -S /usr/libexec/codifix/releases/upgrade.py recover \
  --instance trust --operation "$CODIFIX_UPGRADE_OPERATION" \
  --trusted-signing-fingerprint "$CODIFIX_RELEASE_SIGNING_FINGERPRINT" \
  --action finish
```

`finish` reauthenticates its snapshots, reconciles a crash between the release
exchange and digest replacement, and restarts only the exact candidate. Completed
operations recheck health without restarting the service.
`inspect` reports `recorded_PHASE` from the journal and validates protected inputs;
it does not claim fresh service or database health.

Before the durable `start_intent` event, `recover --action rollback` restores the
previous complete tree and digest, verifies an unchanged compatible database,
and leaves the service **stopped**. Restart the restored service only after
resolving the reason for the aborted upgrade. Incomplete copy generations remain
retained, and no recursive cleanup is performed.

Once `start_intent` is recorded, the candidate might have migrated, published or
accepted work even if startup reporting was interrupted. Rollback is prohibited
from that point; preserve accepted state and use `finish` or a newer compatible
release. Failed health never causes automatic old-binary restoration.

## Offline evidence

```bash
python3 scripts/areas/releases/test_upgrade.py
bash packaging/tests/test-debian-lifecycle.sh
```

Tests sign synthetic non-executable ELF fixtures with disposable GPG keys. A fake
host exercises service calls, schema changes, competing operators, external
writer detection, changed configuration/sandbox hashes, process loss before and
after exchange, credential replacement, startup, and forward-only recovery.
They do not execute signed binaries, contact PostgreSQL, or change live services.
Native ABI, real migration and health verification remain deployment evidence.
