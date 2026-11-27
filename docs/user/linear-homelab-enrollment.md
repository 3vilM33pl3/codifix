# Enroll a peer in the dedicated Linear stack

The local operator remains the TUI requester. A separately enrolled peer can
use the same Linear specialist and its existing project mandate over private
XRPC. The originating agent confirms the prepared draft before it is sent.

A `linear_track` request can include optional `metadata` alongside the title
and description. Supported fields are `priority`, `dueDate`, `assigneeId`,
`stateId`, `cycleId`, `parentId`, `projectMilestoneId`, and `labelIds`. The
confirmation covers the proposed fields, issue, and wording. The specialist
reads back the saved values before returning the issue link. Parent issues
and milestones must belong to the enrolled project.

Create `enrolled-peers.json` with mode `0600` in the operator's state directory
(default `~/.local/state/codifix-linear`). Store the reviewed public peer CA
there as `production-homelab-ca.pem` with mode `0600`. Both must be regular
files owned by the operator. The launcher expects the `production-homelab`
key. Replace each placeholder below with the corresponding reviewed value.

```json
{
  "version": 1,
  "production-homelab": {
    "instance_id": "PEER_INSTANCE_UUID",
    "did": "REVIEWED_PEER_DID",
    "signing_key": "REVIEWED_PUBLIC_SIGNING_KEY",
    "endpoint": "https://127.0.0.1:10451",
    "tls_ca_sha256": "REVIEWED_PUBLIC_CA_SHA256"
  }
}
```

Compare the enrollment against the peer's current instance ID, private DID
record and public verification method. Verify the endpoint with the reviewed
CA before restarting the Linear stack. The launcher writes
`linear-reciprocal.json` to the state directory. The peer's inbound mandate
covers the configured Linear project. The operator's local requester keeps
its own identity.

## Configure the enrolled peer

1. Verify the reciprocal descriptor's `incident_linear.project_id` and
   confirm that the enrolled worker can reach the published Linear endpoint.
2. Add `peer` to the worker's `peers` list and `social_peer` under
   `social.peers.linear`. Check for an existing `linear` alias before editing.
   The worker authenticates outbound using its existing private PDS session.
3. Match the Linear stack's public `peer-root.pem` DER SHA-256 to
   `tls_ca_sha256` and add it to the worker's peer trust bundle.
4. If the incident specialist uses this peer, set `linear` in its profile to
   the complete `incident_linear` object. Review its mandate and permission to
   call the Linear peer. Validate the configs, then verify that one authorized
   incident yields an issue link and an origin-confirmed receipt.

To withdraw access, remove `enrolled-peers.json` and reload the Linear stack.
Recheck the reciprocal descriptor after changing the project, installation,
or ingress.
