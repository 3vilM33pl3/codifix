# Development coding family: dependency build — DRAFT

Five modules, two public smoke tests and three cumulative maintenance stages.
This family belongs only to development. Do not copy it into held-out fixtures.

1. Repair transitive, deterministic dependency traversal and shared prerequisites.
2. Invalidate downstream cache entries from source and dependency-map changes,
   without rebuilding unaffected prerequisites or unrelated nodes.
3. Reject reachable cycles and make failed builds atomic for cache and generation
   state. A retry must still perform the required work.

Ten private black-box probes check positive, cached and rejected outcomes.
The private reference replaces only the planner; other correct implementations
are acceptable. Public tests and contracts stay available to the coding agent;
private probes/reference code do not. Later stages retain earlier acceptance.

Artifact generations are an in-process compilation test double, not actual
compiler output, a distributed scheduler or evidence of production cache safety.
Labels are draft; no provider or held-out evaluation has been authorized.
