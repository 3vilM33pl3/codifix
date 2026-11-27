# Review an OpenBao ceremony

A recovery task lets an enrolled specialist coordinate the custodians of an existing OpenBao installation. The installation operator enrolls the target, controller, independent custodians and private notice recipients in advance. Each custodian retains custody of its own share.

## Respond from the phone

1. Open the waiting notice in your private Inbox. Check the named installation and attempt. The receiver adds the protected custodian link from its own configuration.
2. In that protected page, authenticate and choose **Review request**. Verify the installation, participant, threshold and attempt against your enrollment.
3. If the details match and you approve the recovery, enter your share on the custodian page and select **Approve and contribute**. Choose **Decline** if they do not match. Keep tokens and shares out of chat, task payloads and issue trackers.
4. Check the recovery result. Success requires the enrolled target's native health endpoint to return HTTP 200 with initialized and unsealed state. A custodian acknowledgement records one contribution.

The controller checks private notice enrollment before requesting contributions for an interactive ceremony. On decline, expiry or uncertain submission, the operator inspects native status and custodian records before deciding how to proceed. The [3.1.0 custody and enrollment runbook](https://github.com/3vilM33pl3/codifix/blob/v3.1.0/crates/codifix-agency-openbao/README.md) covers installation checks, independent custody and recovery after an interruption.
