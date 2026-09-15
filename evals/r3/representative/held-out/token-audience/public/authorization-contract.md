# Rowan resource authorization — A10

This fictional verifier accepts a request only after every applicable check:
the trusted issuer and signing key, signature validity, real request-time token
validity, exact target audience, required operation scope and current project
membership. These checks are independent. A trusted issuer's valid signature
does not authorize every service or project.

For this case the request time is stipulated as 2032-06-10 10:00 UTC. The Rowan
gateway's audience is `rowan-gateway`; the inventory service's is
`rowan-inventory`. A scalar audience must equal the target string. A list must
contain the exact target string as a complete member. Prefix or substring matches
do not count. The `inspect` operation requires scope `inventory.read` and current
membership of the addressed project.

Clients request a token for the intended resource through the configured issuer.
Do not edit signed claims, accept a gateway token as an inventory token, or
forward bearer credentials to an unconfigured exchange endpoint. Token exchange
is not available in this fixture. Keep credential bytes and unrestricted claims
out of operational logs; use bounded token IDs and sanitized rejection reasons.
