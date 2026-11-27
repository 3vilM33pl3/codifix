# Specialists and operator workflows

A specialist carries out a task for an enrolled requester. Before using one, check the project, the resource it may act on, and what evidence it must return. A grant can authorize routine work in advance. Some operations also require approval of the exact proposed change.

## Follow one approved task

Suppose you verified a recovery check and want a colleague to add it to the catalogue service's smoke test. Your handoff contains a test result and a short, shareable description. In an enrolled test project:

1. Open the [Linear review terminal](https://codifix.dev/docs/specialists/linear-review). Draft the issue “Add a catalogue search smoke test” and describe the user operation that passed. Include a reference the recipient can access.
2. Submit the draft for review. Check the project in the operator profile and read the proposed issue and wording. If the draft says “check service health” but omits the search, correct the description and review the new proposal.
3. Approve the displayed proposal. The specialist writes the approved fields and reads the issue back from Linear. The terminal displays the verified issue URL when the task completes.
4. Put that URL beside the test result in the handoff. It proves the tracking issue exists with the reviewed wording. The follow-up stays open until its own acceptance check passes.

An unacknowledged response can be retried in the same task. A pending status means the receipt is still outstanding. Check the task before opening a second issue.

## Choose an operator guide

| Workflow | Who initiates it | Decision and completion evidence |
| --- | --- | --- |
| [Enroll a specialist](https://codifix.dev/docs/specialists/enrollment) | The installation operator | Grant the named peer access to one mandate. Inspect its permitted capabilities, resources, budget and expiry. |
| [Track project work in Linear](https://codifix.dev/docs/specialists/linear) | A project operator enables an approved binding | A recorded task start queues the tracking event. Inspect the verified issue link and synchronization state. |
| [Respond to an incident](https://codifix.dev/docs/specialists/incidents) | An enrolled reporter or authenticated alert receiver | Use the configured repair policy. Read the independent health and representative-operation checks before closing the incident. |
| [Renew an SSH host certificate](https://codifix.dev/docs/specialists/ssh-renewal) | An enrolled host monitor starts an expiry event | The CA operator reviews the requested host and principals. The installation receipt records a fresh verified login. |
| [Review an OpenBao ceremony](https://codifix.dev/docs/specialists/openbao) | The configured controller or an authorized recovery task | Each custodian contributes independently. The final check reads the enrolled target's native health and seal state. |
| [Roll out a CA companion](https://codifix.dev/docs/specialists/companion-upgrades) | A certified publisher with a grant from the CA operator | The supervisor verifies the staged artifact, installed version, service health and authenticated return call. |

For participants who want to read or respond to a task, start with [Network and Inbox](https://codifix.dev/docs/network-inbox).
