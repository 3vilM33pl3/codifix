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
networking. Existing gateway/PDS storage is never adopted implicitly.
It is intentionally a **fresh-stack plan**, not an automatic rerun/reconciliation
tool for an already started company stack.

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
