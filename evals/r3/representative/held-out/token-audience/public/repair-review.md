# A10 client repair — pending

Configure the client to request the intended inventory audience from the existing
issuer, with the operation's required scope. Validate its claim metadata without
logging the token. Keep exact-audience, time, signature, scope and current project
membership checks. Do not widen audiences, add missing membership or grant write
scope merely to make a request pass.

Verify valid inventory/lab reads, gateway-audience rejection, near-name rejection,
list-member handling, wrong-scope rejection, cross-project denial and revocation
after token issuance. Use fake identities and tokens; no real credentials need
to be exchanged. Replace the unsafe diagnostic output with bounded token ID and
sanitized reason before operators use it.

The disposable results support the policy, not a completed client rollout. After
10:05, obtaining another valid inventory token still cannot restore alex's lab
membership. Restoration requires a separately authorized membership operation;
none is approved in this review.
