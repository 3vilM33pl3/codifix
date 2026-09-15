# Development coding family: configuration rollout — DRAFT

Five modules and three cumulative stages. This family is assigned only to
development. Public inputs contain the application, complete operational
contract and ordinary smoke path, not private probes or reference patches.

1. Repair precedence and preserve base fallback and staging/live separation.
2. Add sticky deletions, clearing an override and optimistic revision checks.
3. Validate before live replacement; preserve state/revision on rejected or
   failed reloads, allowing an exact draft retry after failure.

The ten hidden black-box probes cover positive operation as well as failures;
neither always rejecting nor ignoring input is a valid implementation. Later
stages retain every earlier probe. The private reference replaces merge/service;
other designs that satisfy the contract are acceptable.

Reload is a deterministic in-process test double. No real infrastructure, model
provider or deployment is contacted. Passing these tests does not establish
atomicity across real filesystem/service boundaries. Labels remain unapproved.
