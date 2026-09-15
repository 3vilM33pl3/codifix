# Authorization interpretation limits

A token can be authentic yet inappropriate for a resource, operation or project.
A gateway health response does not certify an inventory request's authorization.
Token expiry is not the only time-dependent authorization boundary: membership
can change while the signed token remains within its interval.

These selected diagnostics do not establish stolen credentials, malicious actor
intent, successful cross-project disclosure or a complete count of impacted
requests. They contain no credential bytes, production audit census or deployed
patch verification. Do not infer compromise from audience_mismatch alone, nor
absence of all security defects from the negative controls.

The contract is a synthetic evaluation policy, not a replacement for real issuer
documentation. Do not contact arbitrary token exchange services or change keys
as part of answering this case.
