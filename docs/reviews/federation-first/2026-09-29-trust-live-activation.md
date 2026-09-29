# Trust live federation activation evidence

Date: 2026-09-29  
Environment: Olivier's internal CA Trust instance and private company gateway  
Status: federation synchronization complete; deterministic expertise activation in progress

## Established state

The CA-reported result for instruction
`monolith-ca-20260928-trust-federation-activation-v7` records a successful live
Trust federation connection. Trust instance
`cc1bff80-d0e4-4687-9e62-9b8a5fe4bb16` retained and verified all 14 ordered
receipts through the expected signed tip
`sha256:32c40ec7be5bc7e2687e85754469844dfa30979470136baae80c4f1ee8487c09`.
The subsequent worker retry reported no synchronization failures, blocked
publication intents or queued publication intents.

The reported runtime remained Memory Layer `2.1.0-rc.1`, with the application
and PostgreSQL bound to CA loopback addresses. The gateway credential, receipt
trust and negative-authentication probes were installed and verified without
enabling a model, Pi, MCP listener, peer listener or raw PDS access. The CA
reported unchanged Step CA, OpenBao, PostgreSQL 17/main and legacy Memory process
identities. This is operator evidence reported through the private Notion
coordination page; it is not an independent Monolith inspection of the CA host.

The Monolith gateway runtime is configured with all four schema-v3 participants:
authority, Workshop, Homelab and Trust. A live syntactically valid expertise RPC
using invalid credentials returned HTTP 401 on 2026-09-29. The gateway's fixed
TLS edge exposes the exact `/v1/expertise` path. The temporary encrypted-bundle
download listener on TCP 8444 was stopped after the CA confirmed the v7 bundle
installation. TCP 8443 remains available only for the private gateway relay.
The persistent nftables service set still contains the now-unserved 8444 port;
removing that inert rule requires an interactive host administrator action.

## Bounded v8 activation

Instruction `monolith-ca-20260929-trust-capability-activation-v8` is now the only
current CA action. It authorizes a Trust-only write-enabled Memory service and
loopback local-human expertise workflow while keeping MCP read-only and all model
destinations disabled.

The exact reviewed collection is intentionally small:

- collection: `8a65d2ac-1b3f-408d-8151-a4c25dd9b1b8`;
- one reviewed Trust canonical/version:
  `14e013ad-4b0d-4aca-a07b-9e1640220a6f`, version 1;
- deterministic mode only;
- `local_human` output only;
- Workshop and Homelab requester identities only;
- no selected evidence, model destination, service-token caller or external
  model configuration.

The source belongs to the snapshot-bound Trust partition decision ledger. Its
complete history and dependency hashes were unchanged in the 2026-09-27 review.
The activation instruction requires the CA to reject a changed source instead of
substituting a newer version.

The answerer identity is fixed as
`1c2b1f8b-982f-49f8-a392-3cd74292d87b`. It must be provisioned through the
supported expertise-answerer operator with only exact-project read/export
permissions, local `answer-expertise` and access to the one reviewed collection.
It receives no token, global/group authority, other project membership, MCP or
model permission.

## Remaining proof

Completion still requires a CA `DONE` result containing the activated collection
revision/digest and accepted/published/synchronized advertisement identity.
After that result, Monolith must independently:

1. synchronize Workshop and Homelab past the new advertisement;
2. discover the Trust capability from both permitted instances;
3. submit one explicit deterministic local-human question from an allowed
   requester;
4. receive and validate the cited Trust answer; and
5. confirm that an unauthorized requester or destination remains denied.

Until those checks pass, this evidence establishes working federation history,
not a completed live expertise exchange or customer readiness.
