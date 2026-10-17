# Bounded Workshop interaction-grant renewal

`workshop-grant-renewal.mjs` prepares, but never installs or activates, the one
approved seven-day renewal of Workshop's existing three reviewed source versions.
This is not an automatic renewal service and cannot enroll another area, change
the model, add sources or reset spending.

```sh
node deploy/federation/workshop-grant-renewal.mjs questions-renewed-UNIQUE_APPROVED_SUFFIX
node --test deploy/federation/workshop-grant-renewal.test.mjs
```

The operator is deliberately bound to the existing preview's paths, database
container identity, instance/project/service principal and policy revision. It
requires the original profile, application configuration and worker artifact
hashes. It checks current authority, exact source-version/evidence digests,
tombstones, the configured `gpt-5.6-sol` model and limits of20 calls/$2.50 per UTC
day. The database connection is explicitly read-only; source text is hashed
locally, never printed, copied into the new stage or sent to a model.

Only the immutable grant's expiry changes. The new profile digest is different;
old-profile questions cannot silently acquire the new consent. Existing IDs,
source order, evidence bindings, destination, policy revision and sandbox remain
unchanged. No database records—including prior usage or uncertain attempts—are
updated. Current day totals remain aggregated across all policies by area/day.

Preparation requires no unsettled model attempts or unexpired queued questions.
This is a point-in-time preflight, **not** a claim that writers were drained.
Repeat the checks with claims stopped immediately before deployment.

## Prepared renewal —2026-09-19

Private stage:
`/home/olivier/.local/state/codifix-workshop-preview-20260918/questions-renewed-20260919-grant7d`.

- Prepared `2026-09-19T22:42:47.500Z`; expires `2026-09-26T22:42:47.500Z`.
- Manifest SHA-256: `14ad9f571699ff391580864151090111c0929eda0b54580e3e42230c2b41e306`.
- Application-config SHA-256: `9ae8133fff7c70e1dc7f8dbc4381e84ec35646d51f64e024ec13b98a36616f7b`.
- Worker SHA-256: `6953aaea0e69fad53d748e062599d2582024761c7421a2f8f9b1399209b2de00`.
- Queue SHA-256: `624f032e889fb620d223acbf23ff84bbf82766a7d850261c569b5fa6b31ac850`.

The seven-day interval begins at preparation, so delayed activation only shortens
the remaining grant. Do not extend it at installation or silently regenerate a
later expiry. The protected `renewal.json` records the exact interval and checks.
Preparation observed one call charged $0.008172 on2026-09-19, zero reserved cost,
zero unsettled attempts and zero unexpired questions. These are not reset.

The copied real worker accepted the new manifest and staged service configuration
in an offline preflight, then stopped at the explicit missing-database gate. The
same worker rejected the new manifest paired with the old service profile. Both
checks used an environment with **no database URL or provider key**, so they made
no database changes, claims or paid calls.

## Separately controlled activation

1. Stop question claims and drain/reconcile any in-flight work. Recheck exact
   source eligibility, authority and original live-config hash; preserve prior
   artifacts and usage. Do not transfer old/uncertain questions to the new digest.
2. Install the prepared `app.toml` at the preview's existing application-config
   path, and mount this stage's `work.json` at `/interaction/work.json` when
   recreating the reviewed application container. Do not enable
   `[supervisor.model_work]`, add provider credentials to the application or alter
   the database policy/budget.
3. Install the prepared `codifix-workshop-question-worker.service` only after
   validating its exact new queue path and artifact hashes. The copied queue
   still checks the **live** application-config hash and uses the previously
   approved normal credential location; no key is copied into this stage and
   R3 credentials remain out of scope.
4. Verify the application advertises the new grant expiry/source count and that
   the queue is empty. Starting the trusted worker is a separate activation
   action; preparation never starts it. Every new paid question still requires
   its normal exact confirmation. Use fake providers for verification.

If the deploying agent changes any application-config bytes or replaces the
worker binary, the prepared hashes no longer authorize execution. Review and
repin those exact artifacts without extending grant expiry or changing inputs.
Keep all old profiles and completed results available for historical inspection.
At expiry, stop disclosure until a separately approved renewal; there is no
automatic consent extension.
