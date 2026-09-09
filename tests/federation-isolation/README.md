# Executable Stage D network isolation

Run on a Linux Docker host from the repository root:

```sh
node tests/federation-isolation/run.mjs
```

No developer/customer database or external PDS is used. The harness builds the
dedicated Rust gateway and test-only schema provisioning example. It generates
random credentials/keys, a disposable TLS CA/certificate and a Compose project
from the [reference topology](../../deploy/federation/topology.mjs). Two gateway
databases, two instance databases, two unmodified pinned PDSs and TLS proxies
have independent private networks. No host ports are published.

Actual participant containers test authorized fresh signed pages, bad tokens,
cross-space tokens, untrusted TLS roots, raw repository/CAR/event-stream and
general app routes, direct backend bypass, PDS/database IPs on every interface,
DNS names, other local databases and absence of gateway secret/socket mounts.
Each participant reaches its own database as a positive control. Dedicated
gateway startup uses a non-owner, non-superuser, table-limited SQL role.

This is network/packaging evidence, not another live publication proof: synthetic
membership is accepted during provisioning, and PDS credentials deliberately
cannot publish. Real signed publication, independent upstream verification,
reviewed snapshots, cursor recovery, key rollover and provider policy run in
`tests/federation-pds/run.mjs --runtime` and the Rust integration tests. Together
they cover separate failure boundaries without claiming customer deployment.

The fixture pins PDS `d95725b…`, pgvector `cf134a7…` and Caddy `226d1f0…` digests in
`run.mjs`. These are reproducible fixture inputs, not a recommendation that an old
fixture proxy version is suitable for customer release. Review and scan deployment
versions before selecting the release image and repeat these gates.

Cleanup removes only the uniquely named fixture Compose project/volumes, generated
image and temporary secrets. A failed test is a failed gate. Do not disable PDS
SSRF protection, add raw repository routes or grant database-owner privileges to
make it pass. Existing PDS cold-backup/restore and runtime tests remain separate.
