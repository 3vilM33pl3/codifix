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
Resolve the unset health `area_instance_id` against the retained logical
publisher before starting the federation worker. The process health instance ID
and immutable import target are not substitutes for the publisher identity.

## Remaining acceptance

- [ ] Prepare and verify fresh coordinator enrollment and scoped configuration.
- [ ] Deliver credentials through a protected channel; Notion carries only
  public metadata, instructions and sanitized results.
- [ ] CA-local agent activates the approved binding through the existing console.
- [ ] Verify fresh sync plus a deterministic cited exchange and exact replay.
- [ ] Reconcile Notion and the project plan, then complete the plan workflow.

Coordination: [CURRENT ACTION](https://app.notion.com/p/3e78990715f48170a918f3f2c70277c9)
and [RESULT INBOX](https://app.notion.com/p/3e78990715f481cd935ff0a897040e4e).

Latest engineering CI [39146234764](https://github.com/3vilM33pl3/codifix/actions/runs/39146234764)
passed every selected job. Conditional checks were skipped; this does not claim
new coverage from those checks. The published rc.1 artifacts remain unchanged.
