# Selected connections at the T9 reference time

Before the leaf renewal, the deployment bundle contained old leaf plus I.
After renewal, the server's selected bundle contains L only. A file containing
I remains in the release directory, but the active server configuration does
not reference that file. Reload reports configuration parse success.

| Client | Trust/cache | Server presentation | Result |
| --- | --- | --- | --- |
| clean-a | trusts R; no I cache | L only | issuer unavailable; no application request sent |
| warm-b | trusts R; cached I | L only | connection succeeds |
| clean-c diagnostic | trusts R; no cache | L then I | connection succeeds |
| wrong-name diagnostic | trusts R; no cache | L then I; asks for other.internal.invalid | name mismatch |

The clean-c and wrong-name checks use disposable local fixture clients. None
changes the active bundle. No client in this table uses X as a trust root. A
successful reload or cached client is not a clean-client chain test. There is
no evidence of an expired L at the reference time or a compromised private key.
