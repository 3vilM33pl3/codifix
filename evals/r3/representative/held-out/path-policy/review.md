# Held-out coding family: path policy — DRAFT

Five modules and two public starter tests model component-based path authority.
Aliases are an in-memory table, not actual filesystem symlinks; no test opens
host paths or claims a production sandbox. Keep this family wholly held-out.

1. Three probes require subtree component boundaries, normalized descendants,
   rejected parent/percent/relative paths and non-mutating invalid grants.
2. Three probes separate read/write authority, exact normalized revocation,
   duplicate/absent mutations and surviving nested grants.
3. Four probes bind authority to requested and resolved targets, preserve suffixes,
   choose the longest component alias, reject escape/cycles and distinguish eight
   permitted rewrites from a ninth. Alias mutation affects subsequent checks.

Ten private probes stay outside the agent; the private reference must pass all
and every cumulative stage. Starter and no-op controls must fail each stage.
Accept any implementation satisfying the public contract, without source-string
matching. Draft labels need approval; no paid model or calibration run is implied.
