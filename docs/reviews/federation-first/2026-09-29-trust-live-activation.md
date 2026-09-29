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

## V8 review stop and corrected retry

The CA correctly stopped v8 before activation because the instruction described
the ownership ledger's `reviewed_content_sha256` as though it were the digest of
the collection's `MemoryContent` object. Those values use different inputs:

- `bb195e6275660677ef8ce5a994d4f305722db2722d77ce66ea5e7ad3cba65061`
  is the snapshot migration ledger's stable hash of the redacted full-history
  review object;
- `a5780f572e73e0f5ce8146af655512a3a5eaab7bc2dda5f2ec274257614abc0c`
  is the canonical JSON SHA-256 of the exact collection `MemoryContent` object.

Monolith independently reconstructed the latter from the snapshot-bound memory
row and sorted tag rows. It includes integer confidence `960000` parts per
million, rather than the source database's floating representation. The result
exactly matches the CA's independently derived content-object hash. The CA also
reported the expected canonical/version, one-entry snapshot, zero evidence and
exact blueprint policy. This establishes a hash-contract mismatch in the v8
instruction, not evidence of changed source content.

Instruction `monolith-ca-20260929-trust-capability-activation-v9` supersedes v8
and directs the CA to reuse the retained exact draft:

- draft request: `c82918ee-5f3d-4088-8622-c31ce3ae1f0e`;
- snapshot/collection digest:
  `0d7c2fff770ccecac145383269a8ad243946ce52ec887d2ad37b4795ccbac94d`;
- review digest:
  `de65840224e8a1080841934e62f10462f9a9603bbcb00164bbcfd2ed5053172d`.

The v8 run committed no answerer, local grant, collection consent,
advertisement or publication. It restored the Trust configuration byte-for-byte
and left federation synchronized through sequence 14. Therefore the stable
provision/grant request identifiers remain unused and the proposed collection
revision remains 1.
