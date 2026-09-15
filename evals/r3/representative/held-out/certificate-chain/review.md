# Held-out knowledge family: certificate chain — DRAFT

Five fictional sources and four questions. The chain rules, issuer graph and
cache behavior are stipulated; no live endpoint, keys or certificate library
is used. All validity calculations use the explicit 2031 reference time.

1. Clean-a cannot find I after the bundle replacement; warm-b has it cached.
   The leaf is not expired at the reference time.
2. Restore L/I presentation with unchanged trust and verification. Validate actual
   deployment through a clean client and a failing wrong-name control.
3. X, clock changes and a cached success do not replace the missing issuer or
   independent name validation. Transmitted roots do not self-authorize trust.
4. Fleet impact, revocation and key safety are not established by these samples.

Accept equivalent precise explanations supported by citations. Private labels
remain draft and require blinded semantic review. This is held-out fixture work,
not authorization to change live trust, credentials or deployments.
