# CA Trust: activate fresh Codifix federation

Instruction: `ca-codifix-federation-20261018-v1`.

Olivier explicitly approved completing Trust federation enrollment and the
scoped CA application activation with existing permissions. Execute through the
existing console. Keep SSH disabled and Step CA/OpenBao unchanged. No repeated
approval is needed for these steps. No model/provider/agent activation, new
disclosure destinations or public listeners are authorized.

The local Codifix upgrade is complete. Do not reinstall, rerun migrations,
repeat retirement or restore an old database. Preserve the final local-upgrade
receipt and existing recovery set.

## 1. Verify and decrypt the fresh connection package

Preparation can run while the existing service remains healthy with federation
disabled. Use the existing operator identity that owns the CA delivery key.
Primary OpenPGP fingerprint:
`ACDBD1CB5ECB39FCA84439ABC2E27E7B42623A5E`.
Encryption subkey:
`D39EB8070E71832B63E9E4516CA8C0F9CBCB9BF3`.
Only ciphertext is on the existing private LAN drop; do not upload plaintext
or private keys to Notion.

```bash
umask 077
work=$(mktemp -d /tmp/codifix-trust-federation-20261018.XXXXXX)
curl --fail --silent --show-error \
  http://10.22.6.42:8444/trust-fresh-20261018/trust-fresh-connection-v3.tar.gpg \
  --output "$work/connection.tar.gpg"
printf '%s  %s\n' \
  757933a7dc29fbf39aa365552a91e0de73198ff5a26d5ea7240efe6b7beb5853 \
  "$work/connection.tar.gpg" | sha256sum --check --strict -
```

Stop on checksum mismatch. The ciphertext is 3,278 bytes. Decrypt with the
existing recipient key into a private file; never print the plaintext archive:

```bash
gpg --output "$work/connection.tar" --decrypt "$work/connection.tar.gpg"
tar --list --file "$work/connection.tar"
```

Require exactly these regular files: `bundle.json`, `member-trust-token`,
`connection.json`, `worker.patch.json`, `ca.pem`, `README-v3.txt`. Then extract
inside this private directory and verify the public root independently:

```bash
tar --extract --file "$work/connection.tar" --directory "$work" \
  --no-same-owner --no-same-permissions
printf '%s  %s\n' \
  c6d3f87e7c4d935ff84f70e5a23474417751a0ee1a96e24027daa2e8f665952c \
  "$work/ca.pem" | sha256sum --check --strict -
```

Inspect `connection.json`, the patch and README locally. Never print the token.
If the existing decryption key is unavailable, report that exact dependency and
provide only a replacement public encryption key through RESULT INBOX. Do not
change authentication or request SSH access.

## 2. Bind the retained identity and scope

| Field | Required value |
| --- | --- |
| Fresh space | `e9ba9b50-a888-467a-a894-6dc392402dd5` |
| Authority | `c863e242-647d-4f6a-b28f-ccc1612000ce` |
| Trust publisher | `cc1bff80-d0e4-4687-9e62-9b8a5fe4bb16` |
| Project | `b0618015-3923-41ba-a9ae-7607cf51dd2d` |
| Collection / revision | `8a65d2ac-1b3f-408d-8151-a4c25dd9b1b8` / `1` |
| Existing answerer principal to verify | `1c2b1f8b-982f-49f8-a392-3cd74292d87b` |
| Capabilities | Pull, Publish, AskExpertise, AnswerExpertise |
| Output | `local_human` only |

Gateway: `https://gateway.memory.robotmotel.com:8443` (existing endpoint).
Receipt key generation 1, first sequence 1:
`did:key:zQ3shuwZANKFiEY3KZJBDMDoWRPwrDkHpeFSasrVPpi6kCuuk`.

The gateway's schema-3 and addressed-schema-1 fleet entries are compatibility
declarations. They do not authorize adding `agent_ask`, assistant callers or
model destinations to Trust.

Astra verified the pinned rc.1 source: null health `area_instance_id` is expected
when `[area]` is absent. Leave it absent. If already configured, it must match
the federation worker's stable publisher. Preserve immutable import target
`9be66620-5710-4018-a2b8-e1f2ed2a43fa`; it is not the worker identity.

## 3. Stage and activate the application configuration

Use the actual named service's owner and sandbox from the completed upgrade.
Retain current config, worker JSON, old credential and systemd settings privately.
Stage the new token and root at the new paths in `worker.patch.json` as
root-owned, mode0600 source files beneath a protected parent. The named service
uses DynamicUser and systemd credentials; do not grant a persistent dynamic UID
access to source files or weaken the protected-path sandbox. Systemd loads the
sources before applying the service sandbox.

Merge only the patch's supported worker fields into a copy of the retained
worker JSON: space, authority, instance, gateway, receipt_keys, gateway_ca_file
and token_file. For this systemd installation, replace the patch's source paths
with the actual runtime credential paths below before saving the worker JSON:

```json
{
  "gateway_ca_file": "/run/credentials/codifix@trust.service/gateway-ca-20261018.pem",
  "token_file": "/run/credentials/codifix@trust.service/member-trust-20261018.token"
}
```

Systemd does not expand `%d` inside JSON. Save the completed worker JSON to
`/etc/codifix-instances/trust/federation-20261018.json`, root-owned/mode0600.
`gateway_ca_sha256` is external package metadata, not a worker field. Preserve
project/principal bindings, existing browser and local-human restrictions, and
empty model destinations. Do not add callers.

Confirm **COORDINATOR_PUBLISHED_PROVEN** in CURRENT ACTION before starting the
new worker. The exact four-record enrollment preview digest is
`cb7489c403460023871d24c740fb97ca3f877b40eb062aa2f896062e603f8953`.
An accepted or queued state is insufficient. If not published yet, finish
staging and report ready; do not rerun coordinator enrollment from the CA.

Use a brief stop of only `codifix@trust.service` for the final application
configuration/credential switch. Inspect current drop-ins, then add the following
to an instance-specific drop-in, preserving all existing unrelated settings:

```ini
[Service]
LoadCredential=federation.json:/etc/codifix-instances/trust/federation-20261018.json
LoadCredential=member-trust-20261018.token:/etc/codifix/trust-federation-20261018/member-trust-token
LoadCredential=gateway-ca-20261018.pem:/etc/codifix/trust-federation-20261018/ca.pem
ExecStart=
ExecStart=/usr/libexec/codifix/run-instance %i %d --federation
```

An ambient `CODIFIX_FEDERATION_CONFIG` environment variable alone is ignored by
the finite launcher. The explicit flag and named credential are both required.
Keep `IPAddressDeny=any` and protected-path/privilege restrictions. Restore only
the approved gateway destination's actual address and any existing required DNS
access in the unit's allowlist if not already present. Do not reset the allowlist
to unrestricted access or alter the host firewall, public listener or TLS checks.
Use `systemctl daemon-reload`, then start `codifix@trust.service`. Do not restart
PostgreSQL, Step CA, OpenBao or the host.

Verify healthy database/schema85/version3.0.0-rc.1 and fresh-space sync. Keep the
retired cursor23/history untouched and old queues disabled. No automatic bulk
republishing of imported or local knowledge is authorized.

## 4. Restore the existing collection under the fresh space

Inspect actual retained project permissions and local grants; never infer them
from a UUID. Restore only the same project-scoped grants under the new space,
using the normal authenticated grant/collection workflow. Preserve the exact
collection UUID/revision, canonical source versions, content/evidence digests,
requester allowlist and local-human destination. Fresh-space collection/review
digests may change because they bind the new space; original source/evidence
digests must remain exact. Verify the existing answerer
principal has only its previous project permissions and collection grant.

The normal CLI supports
`codifix federation --project homelab --local-human-session collections ...`
for this existing single-user operator
workflow. Confirm the actual slug maps to the required project UUID. Inspect the
installed CLI help and retained local blueprint, draft a fresh-space review,
then activate the exact reviewed draft. Do not bypass authorization with direct
SQL, broaden a collection, select replacement source versions or export its
private content into Notion. If retained source evidence no longer qualifies,
report its IDs/digests and reason without fabricating a current consent record.

Verify publication and current capability discovery. Then post
**CA_READY_FOR_EXCHANGE**, current cursor, collection/revision, principal/grant
confirmation and credential-package digest to RESULT INBOX. The coordinator
will run one previously approved deterministic Workshop-to-Trust local-human
question and exact retry. Keep actual question/answer bodies in private artifacts;
Notion and agents receive status, citation IDs/versions/digests and equality only.

## 5. Complete the result

After the exchange, report a fresh sanitized JSON with instruction ID,
capture time, exact fresh space/publisher, membership capabilities, sync cursor,
zero failures/queued work, collection/revision and grant scope, request ID,
answer/citation digests, exact-replay equality and original origin/version.
Confirm model calls remain disabled, SSH remains masked/inactive, no new public
listener exists, and Step CA/OpenBao remain unchanged and healthy.

Append before `CA_RESULT_END` in
[RESULT INBOX](https://app.notion.com/p/3e78990715f481cd935ff0a897040e4e), preserving
previous reports, and read it back. Mark this instruction complete in
[CURRENT ACTION](https://app.notion.com/p/3e78990715f48170a918f3f2c70277c9)
only when local sync and the cross-host exchange/replay are verified. A staged
configuration, accepted enrollment or running process alone is not completion.
