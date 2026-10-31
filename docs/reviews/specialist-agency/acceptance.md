# Specialist agency implementation acceptance

Date: 2026-10-31. Branch: `feat/specialist-agency`.

The first implementation is an opt-in companion service with an independent
OpenBao controller. No production installation, grant, custodian enrollment,
public profile, live model request or real Linear ticket was created during
implementation. The original Linux desktop checkout remains unchanged.

## Verified

| Area | Evidence |
| --- | --- |
| Policy and discovery | 24 pure-domain tests: exact grants/attenuation, public projection, A2A fixtures, routing, canaries and matched evaluation accounting |
| OpenBao | 13 transport/security tests; separate actual OpenBao test initializes a disposable synthetic 2-of-3 vault, unseals through custodian processes, restarts it sealed and unseals again under unchanged enrollment |
| Project adapter | 25 tests including real HTTP to local GraphQL fixtures, all object types, stable IDs, updates, readback, lost responses, partial effects, manual edits and live read/write admission |
| Runtime | 43 tests with required disposable pgvector-backed integration: scheduled agency, budgets, delegation, private HTTP, model routing, Linear execution/inspection, learning deployment and UI policy |
| Existing persistence/records | 15 store tests and 8 record tests pass; specialist workers and memory workers claim their own job kinds |
| Operator browser | Optional real Chrome/Playwright test passes: no request before login, token only in memory, safe text rendering, cancellation and mandate controls, revoke confirmation and logout clearing |
| Static/build checks | Workspace formatting; strict all-target Clippy on the four new crates; existing service and CLI compile with the new job kind |
| CLI smoke checks | Example configuration validates; matched synthetic comparison executes and explicitly reports that no real model benefit has been established |

The combined normal test command passed **128 tests**, with the native OpenBao
and browser tests skipped by default. Both optional tests were then explicitly
run and passed: **130 tests total**. Some tests exercise several failure cases;
this count is test functions, not a claim about exhaustive correctness.

```sh
export CODIFIX_TEST_REQUIRE_DB=1
# Set CODIFIX_TEST_DATABASE_URL to a dedicated disposable pgvector database.
cargo test -p codifix-agency -p codifix-agency-runtime \
  -p codifix-agency-projects -p codifix-agency-openbao \
  -p codifix-store -p codifix-record --all-targets --locked
cargo test -p codifix-agency-openbao --test ceremony \
  actual_openbao_shamir_two_of_three --locked -- --ignored
# Set CODIFIX_OPERATOR_PLAYWRIGHT_MODULE to an installed Playwright module.
cargo test -p codifix-agency-runtime --test operator \
  browser_private_session_renders_safely_and_controls_only_authorized_same_origin_paths \
  --locked -- --ignored
cargo clippy -p codifix-agency -p codifix-agency-runtime \
  -p codifix-agency-projects -p codifix-agency-openbao \
  --all-targets --locked -- -D warnings
cargo fmt --all --check
cargo check -p codifix-service -p codifix-cli --locked
```

## Review findings resolved

Incremental review and failure tests corrected these specific issues:

- Duplicate physical model/message attempts after uncertain responses.
- Worker route startup and A2A blocking/pagination/media-type semantics.
- Reboot identity enrollment that originally bound recovery to one boot.
- Unmetered project reads and changes to an accepted plan between checking and
  physical admission; final admission now locks the accepted head and job lease.
- Lost source destination restrictions, delegation cycles, remote authority
  continuing after coordinator loss, and lease renewal after a blocking poll.
- Unselected canary fixtures exhausting the real active-task cap.
- Reusable self-reported learning successes; operational JSON validity is now
  separate from externally assessed quality.
- Reversion between deployment assignment and paid send; final admission checks
  the active deployment. Promotion also rechecks all assigned canary tasks for
  completion and independent quality evidence, including unobserved failures.

## Remaining operational acceptance

The source is implemented and locally validated. The following depend on the
actual deployment and are not established by synthetic fixtures:

- Reviewed native CA seal/threshold, storage and identity enrollment; real
  independent custodian failure domains; offline trust and human alert channel.
- A designated Linear test project, actual API scopes and live schema/permission
  checks before standing ticket authority is enabled.
- TLS proxy, deployment accounts, database backup/restore and secret provisioning.
- Measured held-out Codifix workloads comparing deterministic, single-agent and
  specialist-network quality, all-in cost, latency and human intervention.

Linear's lack of conditional updates leaves a documented same-field edit race.
A2A support is the declared non-streaming HTTP+JSON subset, with Codifix-specific
lease/policy data for delegation. No full-protocol certification or general
self-improving-intelligence claim is made. Code deployment, Jira, model-weight
training and live trading remain outside this release.

See the [operator guide](../../user/autonomous-specialists.md) and
[architecture](../../developer/architecture/agency.md) for installation and
boundaries.
