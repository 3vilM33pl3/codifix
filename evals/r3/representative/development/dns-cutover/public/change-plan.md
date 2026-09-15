# Grove build mirror cutover C7

The synthetic name mirror.grove.test has two deliberate DNS views. Company
resolvers return private service addresses; the public authoritative view returns
an external relay. Internal clients must not receive the public relay as a repair
for a broken private view. A successful public lookup tests only that public view.

C7 moves the internal mirror from 10.20.0.10 to 10.20.0.20. The public relay
record is unchanged. The private A-record TTL was 3600 seconds before cutover.
The plan requires lowering it to 60 at least one old TTL before changing the
address. Changing TTL cannot retrospectively shorten an answer already cached
by a resolver under its previous TTL.

Keep the old mirror serving compatible reads for the documented overlap window,
and check resolver answers and successful artifact reads from each required
network segment before retiring it. DNS convergence alone is not proof that
every application or connection has moved.
