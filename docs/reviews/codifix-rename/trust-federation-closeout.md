# Trust federation closeout

Instruction: `ca-codifix-federation-20261018-v1`.

On 18 October 2026 Olivier approved completing fresh Trust federation enrollment
and scoped CA activation with the existing permissions, coordinating through
Notion and keeping SSH disabled. This includes the Trust application changes
needed for activation and a deterministic cited exchange with exact replay.
It does not authorize Step CA/OpenBao changes or new model/agent destinations.

## Completed local upgrade

The CA-local report captured at 17:59:20 UTC confirms Codifix 3.0.0-rc.1,
schema 85, healthy local reads/writes and restart acceptance. Original knowledge,
signed history and 189 historical state files were preserved. The existing
118-table/four-sequence restore was verified. Do not repeat the upgrade.

Final receipt SHA-256:
`eec304fb482b559244a3e60382cefcd2a0c426410da733ebeea472058aa15309`.
CA-local commits: `5007b81`, `760e072`, `e33f660`.

## Approved federation scope

| Binding | Value |
| --- | --- |
| Stable Trust publisher | `cc1bff80-d0e4-4687-9e62-9b8a5fe4bb16` |
| Distinct immutable import target | `9be66620-5710-4018-a2b8-e1f2ed2a43fa` |
| Project | `b0618015-3923-41ba-a9ae-7607cf51dd2d` |
| Collection / revision | `8a65d2ac-1b3f-408d-8151-a4c25dd9b1b8` / `1` |
| Capabilities | Pull, Publish, AskExpertise, AnswerExpertise |
| Destination | `local_human` |

Preserve the retired space at cursor 23. Use fresh enrollment and credentials;
do not resume old queues, republish imported knowledge or widen permissions.
Astra verified the pinned rc.1 source: null health `area_instance_id` is expected
when `[area]` is absent and does not block federation. Leave absent `[area]`
unchanged. If configured, its instance must equal the federation worker's
publisher. The process health instance ID and immutable import target are not
substitutes for the publisher identity. Validate the existing collection tuple
under the fresh space; retained legacy-space rows alone do not activate it.

The coordinator's fresh space is `e9ba9b50-a888-467a-a894-6dc392402dd5`, with
authority `c863e242-647d-4f6a-b28f-ccc1612000ce`. The existing CA OpenPGP delivery
workflow can carry ciphertext over its private LAN drop without enabling SSH
or changing OpenBao. Credentials must remain private; only public key metadata
and ciphertext hashes belong in coordination pages.

## Remaining acceptance

- [x] Prepare and verify fresh coordinator enrollment and scoped configuration.
- [x] Stage the sealed credential package through the existing protected delivery
  workflow; Notion carries public metadata, instructions and sanitized results.
- [ ] CA-local agent confirms package decryption and credential installation.
- [ ] CA-local agent activates the approved binding through the existing console.
- [ ] Verify fresh sync plus a deterministic cited exchange and exact replay.
- [ ] Reconcile Notion and the project plan, then complete the plan workflow.

Coordination: [CURRENT ACTION](https://app.notion.com/p/3e78990715f48170a918f3f2c70277c9)
and [RESULT INBOX](https://app.notion.com/p/3e78990715f481cd935ff0a897040e4e).

Latest engineering CI [39146234764](https://github.com/3vilM33pl3/codifix/actions/runs/39146234764)
passed every selected job. Conditional checks were skipped; this does not claim
new coverage from those checks. The published rc.1 artifacts remain unchanged.

## Coordinator completed — 18 October

Astra reviewed the exact gateway change: append Trust's credential and schema
3/1 compatibility entries, preserving all existing identities and policy. The
gateway was recreated using its existing pinned image; its old stopped container
and protected configuration are retained. The authoritative private container
definition now includes the fresh credential mount for subsequent recreation.

Normal enrollment reached `published_proven` on exact retry with the same record
IDs and digest. Four membership records occupy sequences 13–16; published tip:
`6ef4b82082d6bc9c59505fbc06bc9baba362512024b657f25204490683434519`.
Fresh Trust authentication returned HTTPS200 and the retired credential HTTPS401.
Workshop, Homelab and General remained healthy at cursor16 with zero failures.

Private evidence is under
`~/.local/state/codifix-rename-20261017/trust-enrollment-20261018/`:
`exact-retry.json`, `nonca-healthy.json`, `credential-validation.json`,
`gateway-activation-result.json` and `persistent-runtime-update.json`.

The [console activation handoff](ca-federation-activation.md) is posted in CURRENT
ACTION with the encrypted v3 connection package checksum. Its three shell blocks
were read back exactly and passed syntax validation. The named service's finite
launcher, DynamicUser credential loading and exact gateway egress are explicitly
covered. CA activation and the prepared deterministic exchange/replay are still
pending; coordinator enrollment alone does not close the rename.
