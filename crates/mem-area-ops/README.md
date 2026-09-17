# Private area import operator

`memory-area-import` is an opt-in local operator executable, not a service route
or an automatic deployment. It never discovers configuration, connects to a
source database, runs migrations, creates credentials or starts workers.

## Inputs and preparation

Use an existing mode-0700 directory owned by the operator; all input files must
be owned mode-0600 regular files with one link. Symlinks (including ancestors),
shared writable non-sticky ancestors, changed inputs and existing outputs fail
closed. Root and the same operator UID remain trusted. Do not run this tool
against directories another process under that UID is modifying.

1. Preserve the encrypted backup, restoration evidence and metadata inventory.
   Complete the ownership/provenance review in a separate manifest. A computed
   digest or syntactically valid manifest is **not** human approval.
2. Run `memory-area-import inspect --inventory /protected/inventory.json
   --manifest /protected/reviewed-manifest.json`. Record the normalized
   `inventory_sha256`, `manifest_sha256`, `plan_valid` and blocker count. This is
   read-only; it does not inspect row payloads or connect to PostgreSQL.
3. Collect exact row payloads from the already isolated restore with
   `node scripts/areas/payload.mjs --source-instance UUID --snapshot-id UUID
   --inventory-sha256 INVENTORY_SHA --output /protected/new-payload.ndjson`.
   This collector is deliberately pinned to the four-area restore container,
   not an arbitrary server. It uses a read-only transaction, exact UTF-8 JSON
   bytes and lexical key ordering. The supplied inventory digest is only a
   claimed binding; preparation independently checks every row against it.
4. In a new empty private output directory, run:

   ```sh
   memory-area-import prepare \
     --inventory /protected/inventory.json \
     --manifest /protected/reviewed-manifest.json \
     --payload /protected/new-payload.ndjson \
     --inventory-sha256 INVENTORY_SHA --manifest-sha256 MANIFEST_SHA \
     --output-dir /protected/prepared
   ```

   The tool publishes `archive.ndjson` and `prepared.json` exclusively, after
   file and directory synchronization. The archive contains **every original
   row**, including private raw context and retired runtime state. It must not
   enter a target's retrieval corpus or a repository. The receipt is metadata
   only and binds the exact input files, archive and per-area partitions.
   Retain its printed `prepared_sha256`; back up the archive securely and verify
   that backup before importing. Temporary files retained after interruption are
   not authority to import; inspect them and use a new output directory.

## Explicit target and import

Provision an independent local target with the exact supported migrations
(currently 0056), no application/worker connections and no prior knowledge,
credentials, jobs or federation authority. Migration and writer fencing are
separate operator steps. A target UUID is a binding, not proof of host identity.

Create a private target descriptor using the intended credentials, never through
command arguments or chat. Its strict shape is:

```json
{
  "format_version": 1,
  "instance_id": "REPLACE_WITH_NEW_TARGET_UUID",
  "area": "general",
  "connection": { "kind": "loopback", "port": 5432 },
  "database": "REPLACE_WITH_FRESH_TARGET",
  "username": "REPLACE_WITH_TARGET_OPERATOR",
  "password": null
}
```

`password: null` is only appropriate for intentionally configured local
authentication. An explicit Unix socket is also supported with
`{"kind":"unix","directory":"/absolute/socket/directory","port":5432}`.
Remote hosts, URLs, `.pgpass`, application configuration and all `PG*`
environment overrides are excluded. Loopback/socket authentication and process
isolation must already be secured by the operator. Hash the descriptor with
`sha256sum`; do not print its contents. Then run:

```sh
memory-area-import import \
  --inventory /protected/inventory.json \
  --manifest /protected/reviewed-manifest.json \
  --payload /protected/new-payload.ndjson \
  --inventory-sha256 INVENTORY_SHA --manifest-sha256 MANIFEST_SHA \
  --prepared /protected/prepared/prepared.json \
  --prepared-sha256 PREPARED_FILE_SHA \
  --target /protected/target.json --target-sha256 TARGET_FILE_SHA \
  --operation-id STABLE_IMPORT_UUID \
  --execute --writers-stopped --archive-backed-up
```

The last three flags are explicit operator attestations, not checks that backups
or fences exist. All original inputs and the full archive are reverified before
connecting. Only the selected area's opaque validated partition is imported.
The transactional importer retains its fresh/inactive/schema/identity checks;
automation remains disabled. No auth tokens or provider settings are restored.

Save the metadata receipt. After an ambiguous acknowledgement, reuse the exact
operation ID, inputs and descriptor: an inactive target is reconciled against
all stored rows and receipts rather than blindly imported again. Activated or
changed targets cannot be replayed. Never invent a new operation to bypass a
failure. Errors suppress SQL, row contents, credentials and source paths.

## Verification and limits

Required-database tests invoke the real binary using generated synthetic
databases and protected temporary files. They cover preparation, exact hashes,
artifact tampering, private-file controls, inactive-target replay, complete
archives and reviewed projections. They do not establish human approval,
encrypted off-host durability, host identity, live writer fencing, deployment
activation or CA compatibility. Real-data import remains blocked until those
gates and the reviewed ownership manifest are complete.
