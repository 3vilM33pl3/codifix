# Lantern operations journal — 6 September 2026

Two customer-support groups, Amber and Blue, use separate project spaces. Their
request principals have disjoint project grants. The diagnostic export below is
a retained sample, not a complete access log.

09:14:02 — Amber principal requested “How should dispatch acknowledgement retries
be handled?” with space ID `space-amber`. Authorization allowed Amber access.
Answer-cache lookup `q:98fd` missed. Retrieval executed with Amber's project
restriction. The response cited Amber's dispatch procedure, source version 7.

09:14:08 — Blue principal asked the identical question with space ID `space-blue`.
Authorization allowed Blue access. Answer-cache lookup `q:98fd` hit. No retrieval
SQL ran for this request. The delivered response still cited Amber source version
7. Blue's principal did not have an Amber grant.

09:16:31 — An operator repeated Blue's query with the answer cache bypassed.
Retrieval used Blue's project restriction and returned Blue procedure version 3.
The audit sample contains no cross-project row from that uncached request.

The sampled Amber receipt's signature verifies against the enrolled Amber key.
The unrelated cache-eviction counter rose later at 09:19. The export contains no
evidence that eviction preceded either response. Full population-level request
counts and downstream copies are not available in this packet.
