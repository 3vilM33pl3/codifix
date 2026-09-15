# Cedar client trust contract — T9

This is a fictional certificate-validation model. A connection needs a valid
chain from the presented leaf through the required intermediate to a configured
trust root. Name, validity interval and signature-chain checks are independent
requirements; satisfying one does not waive the others. Validation time for this
case is fixed at 2031-04-12 12:00 UTC.

Managed clean clients trust root R, do not cache intermediates and do not fetch
missing issuers over the network. The server must present leaf L followed by
intermediate I. Root R is installed by the client's separate trust policy, not
made trusted merely because a server transmits it. Untrusted replacement roots
require a separate trust-change process, not a connection-time override.

T9 forbids disabling verification, ignoring name mismatches, installing the leaf
as a trust anchor, or silently retrying with an insecure client. Keep failures
explicit and sanitized. This fixture does not implement cryptography or describe
all real TLS-client chain-building behavior.
