# Adopt existing project previews into a private company topology

`companyPreviewAdoption(company, bootstrap, adoption)` is a **pure planner**.
It regenerates the validated [bootstrap topology](company-bootstrap-topology.md)
and removes all generated member apps, databases and Web edges. It neither starts
duplicate blank instances nor inspects, modifies or attaches existing ones.
There are no Docker calls, filesystem writes, credentials or provider requests.

The new stack retains the gateway/PDS/operator infrastructure. Each adopted
member gets one **fresh, internal, IPv4-only federation network** whose sole new
service is `gateway_edge`. The gateway backend never joins existing app/database
networks. The returned `docker network connect` argument arrays name exact
reviewed container IDs, not a discoverable container-name prefix.

## Inputs and output

Pass the same explicit `company` and `bootstrap` inputs as the bootstrap planner.
Every configured member requires exactly one binding; unknown, omitted or reused
members/containers are rejected. Example adoption shape (IDs are placeholders):

```js
const adoption = {
  schemaVersion: 1,
  members: [
    { member: 'workshop', containerName: 'memory-workshop-preview-app', containerId: '1'.repeat(64) },
    { member: 'homelab', containerName: 'memory-homelab-preview-app', containerId: '2'.repeat(64) },
  ],
  inventory: {
    containers: [
      { name: 'memory-workshop-preview-app', id: '1'.repeat(64), networks: ['workshop-preview'] },
      { name: 'memory-homelab-preview-app', id: '2'.repeat(64), networks: ['homelab-preview'] },
    ],
    networks: ['workshop-preview', 'homelab-preview'],
    volumes: [],
  },
};
```

Supply a complete, freshly inspected name/ID/network/volume inventory for the target
Docker host, including unrelated names for collision checking. The planner
cannot prove that supplied inventory is complete or current. It refuses an
existing network or volume with any generated new-stack name, retained-service
container name collisions, shared member networks and default `bridge`/`host`
networking. Existing gateway/PDS storage is never adopted implicitly. It is a
**fresh-stack plan**, with only the explicit staged gateway-DB exception below;
it is not an automatic rerun/reconciliation tool for an already started stack.

The output keeps `compose`, retained `proxyFiles`/`requiredFiles`, and gateway/PDS
endpoints. Removed member UI URLs and database names are not advertised. Member
DB credentials, app state volumes, app config mounts and member TLS files are
removed; gateway-owned per-member credentials remain gateway-owned.
`adoption.attachments` pins the member, current container name/ID/networks, exact
new network, expected Compose labels, sole new peer, gateway URL and command
argument array. `reviewSha256` binds the artifact and reviewed inventory digest.
Input order does not change it; recreating an app changes its ID and therefore
requires a new plan. The source inputs are not mutated.

`installed`, `liveInventoryVerified` and `attachmentsExecuted` remain false.
`enrolledMembers` and `approvedCollections` remain empty. The new-stack
`externalProviders: false` posture does **not** assert that an existing app has
no approved model destination: existing model policy is unchanged and uninspected.
This planner adds no provider credential, egress network or autonomous worker.

### Explicit staged gateway database exception

An operator may have already provisioned the dedicated gateway database before
the rest of the stack. The one allowed staged-backend descriptor is optional:

```js
adoption.preparedGatewayDatabase = {
  containerName: 'memory-company-gateway_db-1',
  containerId: 'd'.repeat(64), // replace with the exact inspected ID
  networkName: 'memory-company_gateway_database',
  alias: 'gateway_db',
};
adoption.inventory.containers.push({
  name: 'memory-company-gateway_db-1', id: 'd'.repeat(64),
  networks: ['memory-company_gateway_database'],
  networkAliases: { 'memory-company_gateway_database': ['gateway_db', 'memory-company-gateway_db-1'] },
});
adoption.inventory.networks.push('memory-company_gateway_database');
// Include the actual occupied volume inventory, even though this plan will not own it.
adoption.inventory.volumes.push('memory-company_gateway_db_data');
```

The container and network names must be the exact generated names for this
company project; the DNS alias is fixed to `gateway_db`. The supplied complete
inventory must show that exact container on **only that network**, with the
required alias, and no other container on that network. It cannot be a member
app or a database from a different project. Missing, changed or ambiguous
bindings fail closed. Container network aliases are bounded optional inventory
metadata, required for this staged database and included in the review digest.

The planner removes the generated `gateway_db` service, owner-secret requirement
and DB volume declaration. Existing DB credentials, configuration and storage
remain untouched and outside this plan's ownership. Only `gateway_database`
becomes `{ external: true, name: ... }`; only the new `gateway` service joins it.
No member, gateway TLS edge or PDS service may join. All other container,
network and volume collision/freshness checks remain in force.

Compose cannot enforce `internal` or IPv6 settings on an external network.
`adoption.preparedGatewayDatabase` therefore reports **expected**, not verified,
Compose labels, internal/IPv4 settings, sole current container ID, no published
ports/host networking and new service.
Immediately before deployment, independently inspect these attributes and the
alias, exact container ID/name, database/config bindings, health, role/schema
receipt and backup evidence. A supplied JSON descriptor is not live verification.
Refuse mismatches; do not repair them automatically or expand this exception.
Preserve the original staged provisioning artifact for lifecycle/backup/recovery.
Do not use `--remove-orphans` or enable `COMPOSE_REMOVE_ORPHANS` against the adopted
Compose project: the separately owned database is deliberately absent from the
new stack's service list. The
new stack must never stop, recreate, migrate or delete it implicitly.

## Required operator gates — not performed by the planner

1. Record and verify protected backups of both existing databases, app config,
   identity/credential material and any relevant pending-work state. Record the
   restore limitations; do not claim an untested restore. Preserve prior app
   artifacts and the exact rollback configuration.
2. Review the existing app identities, database bindings, schema/build versions,
   current credentials, runtime settings and network isolation. Do not substitute
   a database container for an app or copy one project's credentials to another.
3. Prepare separately reviewed instance federation configuration, TLS trust and
   member-specific credential mounts. Docker cannot add new env or bind mounts
   through network attachment: deliberately **recreate only the affected app**
   using its reviewed existing state/configuration and original database. Preserve
   model destinations, budgets, provider secrets and worker enablement unchanged.
4. Inspect the recreated running containers and regenerate this plan using their
   new IDs. Review the exact digest. A stale plan is not permission to select a
   same-named replacement. Protect against inventory drift before every effect.
5. Provision the new stack separately. Immediately before attachment, verify the
   intended container's ID/name and original networks; verify new network
   ownership labels, `internal: true`, IPv6 disabled and exactly the gateway TLS
   service as peer. Refuse any unexpected existing network or attached process.
   Never attach the gateway to a project's existing database network.
6. Execute each exact reviewed attachment only after those gates. Recheck local
   app/database health, existing routes/model-egress restrictions and negative
   cross-project/raw-PDS reachability. Attachment itself does not enable federation
   in a running service or grant membership, a collection, disclosure or model use.
7. Complete separate authority enrollment, local answerer grants and explicit
   collection review before actual exchange. Record evidence and rollback actions.
   Later app recreation loses ad-hoc attachments: inspect and deliberately
   reconcile them; never replay an old container-ID command automatically.

Run the pure tests without Docker, credentials or network access:

```sh
node --test deploy/federation/company-preview-adoption.test.mjs
```

These tests establish plan shape and refusal behavior, not live attachment,
database recovery or cross-instance knowledge exchange.
