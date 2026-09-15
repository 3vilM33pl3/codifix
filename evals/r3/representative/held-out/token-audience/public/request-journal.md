# Selected request and code observations

At 10:00 a client configured for the gateway reused t1 for inventory `inspect`
on lab. The gateway health request had succeeded, but the inventory request
failed `audience_mismatch` before reading project records. No provider outage or
expired credential is recorded.

In disposable checks at 10:00, t2/lab and t4/lab pass; t3/lab fails audience,
t5/lab fails scope and t2/game fails membership. At 10:06, t2/lab fails current
membership after the 10:05 revocation. Successful earlier authorization is not
cached as a continuing grant in those checks.

An unreviewed patch k8 proposes skipping audience validation when the issuer and
signature are valid. A different patch treats `rowan-inventory-admin` as a prefix
match for the inventory service. Neither is deployed. A support script currently
prints the entire authorization header on failure; it has not been run in these
tests, and A10 forbids using it as the diagnostic path.
