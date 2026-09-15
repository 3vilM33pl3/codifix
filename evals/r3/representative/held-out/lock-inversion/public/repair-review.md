# L15 repair proposal — pending

Use one C-then-I order for every path that needs both, including compact. Review
helpers and callbacks for hidden acquisitions. Do not invoke reentrant or
unknown callbacks while holding locks. For callback-derived updates, snapshot,
unlock, invoke and revalidate the source generation before the atomic apply.

Use deterministic barriers to reproduce the a/b interleaving, then verify both
operations complete under the corrected order. Test callback reentry without
held locks and a changed generation before result apply. Assert catalog/index
agreement on success and unchanged projections on rejected stale results.

Timeouts may improve diagnostics but cannot authorize partial publication.
Changing lock types or removing synchronization needs a separate correctness
design; this review approves neither workaround. No completed rollout or
performance improvement is recorded.
