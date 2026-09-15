# Answer cache contract, revision 3

Approved 28 August 2026 for the multi-space rollout. Replaces the revision-1
single-project cache card. This contract concerns completed answer reuse, not
the storage of the underlying memories.

A cache identity includes normalized question, stable space ID, effective
authorization fingerprint, corpus revision, answer mode and model configuration.
A display name or HTTP route is not a substitute for the stable space identity.
Results from an uncached retrieval remain subject to project restrictions before
candidate limits. Do not combine independently scoped candidate sets afterward.

Before delivering a cached answer, recheck current authority and the eligibility
of its cited source versions. A changed authorization fingerprint or withdrawn
source invalidates the reusable result. Expiry is an additional freshness bound,
not the mechanism that separates spaces or grants access.

Origin receipts identify the publisher and exact source version. Their signature
does not confer a recipient grant, validate a cache lookup, or establish the truth
of an answer. Keep citation identity attached to the content it actually names.
