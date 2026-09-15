# Synthetic release selection

| Component | Selected | Origin/version | Class | Attribution | Text digest | Source offer |
| --- | --- | --- | --- | --- | --- | --- |
| alder | yes | org-a/v1 | N | Team A | hShared | not required |
| birch | yes | org-b/v2 | N | Team B | hShared | not required |
| cedar | yes | org-c/v3 | S | Team C | hC | missing |
| spruce | no | org-s/v4 | S | Team S | hS | offer-s |

hShared's exact text is `Shared fictional notice`; hC's is `Cedar fictional
notice`; hS's is `Spruce build-only notice`. Digest labels stand for reviewed
content bindings in this test, not actual cryptographic hashes. The selected
release is a/b/c (alder/birch/cedar); spruce is a local build tool, not distributed.
