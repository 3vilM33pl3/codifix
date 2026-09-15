# Synthetic certificate inventory

Signatures and identifiers below are stipulated fixture facts, not real keys.
The expected service name is `cedar.internal.invalid`.

| Certificate | Subject / allowed name | Issuer | Valid interval |
| --- | --- | --- | --- |
| L | cedar.internal.invalid | I | 2031-04-01 through 2031-05-01 |
| I | Cedar Intermediate | R | 2030-01-01 through 2032-01-01 |
| R | Cedar Root | R | 2029-01-01 through 2034-01-01 |
| X | Birch Root | X | 2029-01-01 through 2034-01-01 |

L's signature verifies under I; I's verifies under R. No L-to-R direct signature
exists and X does not issue I. Clean clients have only R in their trust store.
The current leaf fingerprint label is leaf-new; the previous leaf was leaf-old.
No private-key material, revocation status service or real DNS lookup is present
in this case. The interval checks use the T9 semantic time, not today's date.
