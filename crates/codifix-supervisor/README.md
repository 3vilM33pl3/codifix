# Supervisor domain contracts

This crate is an in-memory, deterministic foundation for the four-area supervisor.
It does not run Pi, call providers, verify signatures, persist state, install
releases, or establish a sandbox. Passing its tests does not establish any of
those runtime gates.

The contracts cover:

- disabled-by-default, revision-bound policies using exactly `gpt-5.6-sol`;
- four fixed area allocations of $2.50 and 20 calls per UTC accounting day;
- integer USD micro-unit reservations, retained uncertain usage, exact settlement
  and stable physical-attempt IDs (duplicates never authorize another send);
- observed/proposed/authorized/queued/running/verified action progression,
  explicit failure/denial/cancellation/expiry and uncertain-effect reconciliation;
- leases with monotonic fencing, rejecting duplicate, expired and revoked claims;
- exact standing-question scope and text evidence, separate from F1 human consent;
- Homelab-only automatic maintenance eligibility at 02:00–03:00 Europe/London,
  with local signature/compatibility/backup/channel/idle checks.

`ActionKey` is the persistent action uniqueness key. Provider request IDs identify
individual physical sends. IDs survive restarts and event redelivery. New policy
revisions cannot silently reuse old authorization. Reusing an ID with different
provider terms fails. Current mutable aggregates and authorization outputs cannot
be deserialized from untrusted JSON; serializable records support inspection.
Storage must enforce provider-attempt ID uniqueness across every area/day, not
just inside one daily ledger. Pausing a policy is reversible; revoking a revision
is permanent, including after a subsequent resume request.

The runtime must provide atomic durable storage and recovery, exactly one budget
authority per area/day, current policy reads, fenced effect boundaries, bounded
provider limits/pricing and usage evidence, local signature verification bound to
manifest bytes, and IANA Europe/London conversion. The four allocations imply the
$10 fleet cap only under that single-authority rule. Old-day uncertain costs stay
in their originating ledger. There is no budget borrowing, automatic HTTP retry,
model fallback, or direct transition from uncertain work to another execution.

The initial contracts intentionally expose no storage rehydration API: production
recovery must be designed with the durable adapter, not by replacing a ledger with
an empty one. A signed release notice is data; only locally established evidence
may populate `UpgradeContext`, and eligibility is not an installation capability.

Validate with `cargo test -p mem-supervisor --all-targets --locked`.
