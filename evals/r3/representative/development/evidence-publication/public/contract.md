# Reviewed disclosure contract

Every publication must contain the bytes frozen at review time. Editing the
working capture afterward must not change an existing review. Review identities
are immutable. A review names the authenticated publisher; another publisher
cannot borrow it, even if both have grants.

The first accepted version is 1. Each subsequent publication must use the next
version. A correction refers to the current published base and preserves its
original author. Record the correcting editor separately. Approval of a stale
proposal is rejected; prior versions and their attribution never change.

Check the current grant immediately before sending. Revocation after review
blocks a new publication but does not erase accepted history. A before-send
fault has no publication effect. An after-commit acknowledgement fault returns
uncertain, but the logical publication exists. An exact request retry returns
duplicate without another publication; changing review or publisher under the
same request ID is rejected. A duplicate receipt is metadata, not new disclosure.

An old design note suggested rereading the working capture at send time to save
memory, and treating the editor as the original author. That proposal was
rejected: neither behavior preserves a reviewed provenance chain.
