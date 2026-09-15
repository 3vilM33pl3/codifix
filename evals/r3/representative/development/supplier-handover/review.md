# Development scenario: supplier handover — labels DRAFT

This is one development family with four questions, not four independent
scenarios. Nothing here approves held-out labels, provider spending or product
quality. Public documents are prepared context; this review must never enter
an answering agent's context. All informed conditions get the same public facts.

1. **Explain the two postings and identify which retry instruction should govern
   the next action.** Require the committed-at-10:03:12 observation, the later
   acknowledgement timeout, the new request ID and request-ID deduplication trace.
   The explanation must distinguish a failed acknowledgement from a failed
   settlement and select revision 7 over the obsolete printed card. Cite the
   incident and current runbook; do not attribute the incident to the cache warning.
2. **Does the inspected configuration eliminate the replay risk?** No: the stable
   key is improved, but 72-hour retention is shorter than the 120-hour supported
   replay interval. Replays after deduplication expiry can evade that protection.
   Require deployment and runbook evidence; do not claim that another duplicate
   was observed or invent a mandated safety margin beyond the documented contract.
3. **What should Northbank do before normal operation resumes, and who owns the
   correction?** Reconcile the ambiguous settlement with Copper Relay, avoid blind
   retry, propose the retention correction, obtain origin-owner review and verify
   delayed-duplicate behavior across the supported interval. Preserve prior
   provenance. Accept either longer retention or a genuinely renegotiated smaller
   replay interval with review and verification; do not silently assume the latter
   happened. Cite the current runbook, configuration and ownership receipt.
4. **How much money was lost and how many customers were affected?** The packet
   does not establish either value. A correct answer refuses to invent them while
   distinguishing the observed two postings for one settlement from population
   or monetary totals. Cite the incident's evidence limits. Do not infer zero loss.

Accept equivalent wording and defensible intermediate reasoning. Unsupported
precise amounts, counts, extra causes or claims that deployment was validated
fail the relevant rubric. Automated substring matches are not sufficient for
these labels; unresolved judgments require blinded review.

The four rubric contracts are now executable through `r3_score` and are stored
under `private/q1.json` through `private/q4.json`. They require genuine human
decisions for semantic scoring; test ballots are not approval. Keep them out of
agent inputs along with this document.

The pack still needs corpus/handover preparation parity checks, full campaign
binding and Olivier's review. It does not replace the complete
10-development/25-held-out scenario requirement.
