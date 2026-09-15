# Held-out knowledge family: token audience — DRAFT

Five synthetic sources and four questions. Tokens are metadata labels only; no
credentials, actual issuer or token exchange operation is involved.

1. Authentic t1 has the wrong resource audience. Gateway health does not grant
   inventory authorization and the recorded rejection precedes data access.
2. t2/t4 pass lab at 10:00; t3 fails exact audience and t5 fails required scope.
   t2 never grants game membership. No implied scope hierarchy is specified.
3. Correct client audience without weakening checks. Membership revocation at
   10:05 still denies t2 at 10:06; another token cannot restore membership.
4. Selected denials do not establish theft, disclosure, complete impact or global
   security. Logging must exclude authorization headers and token contents.

Accept equivalent precise explanations with citations. Blinded semantic review
must approve these private draft labels; they are not model context, calibration
inputs or permission to alter live identity policy.
