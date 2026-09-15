# T9 repair review — pending rollout

Restore the selected presentation bundle to L followed by I; validate its actual
selection and reload it through the normal deployment procedure. Keep client
trust root R and all verification checks unchanged. Test a clean client with no
issuer cache, the expected service name and the fixed validation time. Also
retain a wrong-name negative control to prove verification was not disabled.

The disposable clean-c check supports the proposed chain repair, but does not
prove the active service has adopted it. Capture the post-reload presented chain
and clean-client outcome before closing the incident. Keeping I somewhere in a
release directory is not enough if the configured bundle omits it.

Do not rotate keys, replace trust roots, move validation time, or widen network
egress to issuer-fetch endpoints as an automatic workaround. Those are separate
changes requiring evidence and approval. This review records no such approval
and no completed production repair.
