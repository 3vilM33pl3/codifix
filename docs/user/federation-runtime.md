# Federation runtime: controlled integration environments

Stage C provides opt-in durable publication and synchronization, **not a released
customer federation product**. The wire namespace is unpublished. Do not enable
this against customer data before the [D/E and release gates](../reviews/federation-first/README.md)
pass. Native Spaces alpha is not used.

## Enable deliberately

`memory service run` starts no federation tasks by default. An administrator may
set `MEMORY_LAYER_FEDERATION_CONFIG` to an absolute JSON file path. Each service
process runs one configured role/space. IDs are stable, non-nil UUIDs selected at
enrollment, not regenerated on restart. Use separate PostgreSQL databases for
each instance and the gateway; migrations apply on ordinary service startup.

An instance configuration has these fields (replace all examples with enrolled
identities and an explicitly configured test deployment):

```json
{
  "role": "instance",
  "space": "00000000-0000-0000-0000-000000000001",
  "authority": "00000000-0000-0000-0000-000000000002",
  "instance": "00000000-0000-0000-0000-000000000003",
  "project": "00000000-0000-0000-0000-000000000004",
  "principal": "00000000-0000-0000-0000-000000000005",
  "gateway": "https://gateway.federation.example.com",
  "token_file": "/run/secrets/memory-gateway-token"
}
```

The project/principal must exist locally. Starting the worker neither grants
remote capability nor enrolls a memory. Before sharing, a trusted administration
path must use `set_local_grant`, authenticate the review principal, compare
`preview_source`, and call `review_and_share` in a transaction. This opts the
selected stream into future content/state updates using **only the reviewed
evidence references**. A fresh explicit review is needed to change that evidence.
These APIs are currently in-process `mem_service::federation` interfaces; the
Web/CLI/TUI enrollment/review/proposal screens are still E work. Do not bypass
local permission checks with raw HTTP-to-repository API exposure.

The gateway configuration is separate:

```json
{
  "role": "gateway",
  "space": "00000000-0000-0000-0000-000000000001",
  "authority": "00000000-0000-0000-0000-000000000002",
  "listen": "127.0.0.1:4055",
  "pds": "https://pds.federation.example.com",
  "repository": "did:web:space.federation.example.com",
  "token_file": "/run/secrets/memory-pds-token",
  "credentials": [
    {
      "instance": "00000000-0000-0000-0000-000000000002",
      "token_file": "/run/secrets/memory-authority-token"
    },
    {
      "instance": "00000000-0000-0000-0000-000000000003",
      "token_file": "/run/secrets/memory-gateway-token"
    }
  ]
}
```

Gateway bearer credentials identify enrolled **instances**, not human authors.
An authority credential can submit explicit membership records at `/v1/accept`;
merely listing a member token does not grant Pull or Publish. `/v1/pull` requires
current Pull membership and returns only the contiguous published prefix.
PDS credentials must never be distributed to participant instances. The listener
is HTTP and requires a reviewed private-network/TLS termination arrangement;
the outbound clients require HTTPS. Prevent all raw PDS/CAR/firehose bypasses.
Those deployment controls are not installed by this setting.

Token files must be private (0600 on Unix), 32–4096 bytes. Configuration rejects
unknown fields, invalid IDs and unsafe URL shapes. Endpoints are pinned origins,
not discovered from imported records; redirects and environment proxies are
disabled. `disposable_test_mode: true` permits fixture HTTP only in debug builds;
release service builds reject it. No automatic session refresh is implemented:
expired PDS credentials block work until an operator replaces the private token
file, restarts the role and explicitly retries blocked work. Do not skip a blocked
publication to make a later cursor appear healthy.

## Observe and recover

The storage APIs expose separate `DeliveryStatus` observations:

| Observation | Meaning |
| --- | --- |
| Queued | Local intent committed; gateway has not accepted it |
| Accepted | Gateway committed authorization, reducer result, order and PDS work |
| Published | PDS contains the exact immutable record with matching URI/CID |
| Replicated | One named receiver committed its inbox, projection and cursor |

`intent_status`, `GatewayStore::status` and `ReplicaStore::status` report their
own observation; none implies every participant has received the record. Frozen
`federation_source_changes` with `prepared = false` are also queued work, awaiting
conversion to record envelopes. Operational SQL inspection must be scoped to the
administrator's configured space/instance/project and avoid dumping payloads:

- Source: `federation_source_outbox` records `attempts`, `available_at`, `blocked`,
  `last_error` and acceptance receipt. `retry_source_work` rechecks local authority.
- Gateway: `federation_publish_outbox` records lease, attempt and blocked state;
  `GatewayStore::retry_blocked` restarts the same immutable head after remediation.
- Receiver: `federation_replicas` records `sync_failures`, `retry_at`, `sync_error`
  and its applied cursor. Worker warnings are sanitized; they do not include tokens.

Local PostgreSQL work stays usable while the gateway/PDS/peer is unavailable.
Receiver pages and projection/cursor commits are atomic. A gap or corrupt batch
does not advance the cursor. An authorized recovery path can call
`InstanceSync::resynchronize` to fetch a complete paginated history and atomically
rebuild the projection, but it cannot truncate or fork the known prefix. This
low-level API requires the caller to recheck local read permission, just like
`catch_up` and snapshot reads; the normal `cycle` does this itself.

Reference replay is O(history), with a 16 MiB full-resync limit and 32 records per
page. Backoff and source/PDS retry budgets are durable; exhausted or invalid work
needs intervention. This is not a scalable checkpoint implementation or a complete
disaster-recovery system. Revocation denies later gateway access/publication but
does not erase offline copies. Administrators and admitted recipients can read
shared plaintext. Runtime signature verification, model-forwarding restrictions,
retention/legal holds, browser parity and production backup recovery remain gates.

For synthetic verification, see [the PDS harness](../../tests/federation-pds/README.md)
and [the transaction contract](../developer/architecture/federation-durability.md).
