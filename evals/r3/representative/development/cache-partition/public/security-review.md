# Scope incident review — 6 September, 10:00

The retained journal demonstrates one response delivered to Blue that cited an
Amber source version Blue was not entitled to receive. It does not establish how
many other recipients, responses or downstream copies were affected. The partial
export is insufficient for a population total or a claim of no further exposure.

Immediate containment approved by the service owner: disable completed-answer
cache reuse and purge the affected reusable entries, while preserving the
incident audit material. Do not delete authoritative source memories to disguise
the incident. Investigate additional recipients from retained audit evidence;
this approval does not declare that investigation complete.

Before re-enabling reuse, validate the active revision-3 implementation, repeat
the same question across independently authorized spaces, change grants and
withdraw a cited version, and verify that stale cached results are not delivered.
Check citation space/version attribution as well as HTTP response status. Record
the deployed build and inspected configuration with those results.

The sampled signature is valid. The packet supplies no evidence of a stolen key
or forged receipt. Rotating keys is not the demonstrated cache-partition repair;
new evidence of compromise would require a separate response. Reducing TTL alone
still permits a wrong-space hit during the remaining lifetime.
