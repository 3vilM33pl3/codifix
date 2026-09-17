# R3 development continuation with increased call ceiling

## Authority and scope

Olivier requested: “Continue with increased call count.” The execution checkpoint
succeeded at `2026-09-17T06:04:35Z`. The bounded increment is **500 additional
calls**, raising the aggregate ceiling from 500 to **1,000 calls**, not granting
a new dollar or token budget. Preserve the original aggregate **US$10** and
**10,000,000 tokens**, including all previous uncertain allowances.

Starting reconciliation from the eleven earlier packs: 493 settled and seven
uncertain calls; 3,294,544 tokens and 3,459,780 micro-USD used/reserved. Thus the
new campaign may use at most **500 calls / 6,705,456 tokens / US$6.540220**.
Uncertain allowances are not confirmed charges. Prior packs, receipts, fences and
failed/partial results remain unchanged.

Use the same approved two synthetic development families, labels, five
conditions, three repeats, models, reasoning settings, prompts and isolation.
Retain the 300-second task deadline and conservative coding allocation; its
limitations still apply. Reuse the dedicated version-pinned secret without
changing credentials. Only owned disposable services may be created or removed.
No held-out execution, official evidence or customer deployment is authorized.

## Execution

Prepare a new immutable pack from the tested runtime, including the committed
reduced-request-allocation fix. Verify source/label pins against the reviewed pack
before recording the existing label approval against the new manifest. Record
the actual renewed development authorization with the remaining ceiling, verify
runtime/approval gates, then launch once on a fresh disposable database.

On interruption, retain evidence and reconcile provider journals before any
further action. Completion of development execution is not completion of R3:
human adjudication and the separately authorized official campaign remain open.

### Frozen continuation 5

Pack: `/data/tmp/r3-continue5-20260917`; manifest:
`b43cd0958100064339d6d37e7a84534be1eae4d1f071b22b250782903ee979f7`.
All 29 nonconfiguration asset pins, cases, model, seed, repeats, semantic time,
provider profile and per-task/preparation bounds match the previous reviewed
pack. The only campaign-limit change is the authorized reconciled allowance.
Runtime integrity and label/development approval preflight pass without provider
calls. The latest reduced-request-allocation fix is present in the frozen runtime.

Owned disposable PostgreSQL: `memory-r3-continue5-20260917`, container ID
`948d97f2b88c9625f99d099753530868b128d74b5b34a61f23c5493b50a76a94`,
owner label `continuation-20260917`, loopback port 55439 and tmpfs data. Existing
services are not modified.

## Outcome: complete development execution, review still required

The launcher completed **75/75 executions** and exited 2 as designed for pending
review/quality gates. Its report records `execution_valid: true`,
`pending_knowledge_reviews: 60`, and `harness_valid`, `official_evidence` and
`quality_passed` all **false**. This is the first complete live development
calibration in this sequence, not completion of R3 or a product-quality pass.

- **Knowledge:** 60 answer executions await genuine blinded human decisions.
  Fifty-three carry automated critical flags; those flags and unreviewed answer
  scores are not adjudicated factual-quality measurements.
- **Coding:** 0/15 complete sequence executions succeeded. These are three repeats
  of one repository family under five conditions, not 15 independent tasks.
  Seven first-step checks and one third-step check passed diagnostically; no
  second step passed. Child results must not be counted as successful tasks or
  added again to execution cost. The conservative time allocation remains a
  material limitation; do not claim these measure unrestricted model competence.
- No labels, prompts, model settings, retrieval algorithms, deadlines or scoring
  rules were changed to improve results. No held-out data was executed.

### Spending reconciliation

This run made **167 settled calls**, with **174,662 input + 194,873 output =
369,535 tokens** and **959,797 micro-USD (US$0.959797)**. Preparation accounts for
37 calls / 2,910 tokens / 82 micro-USD; execution accounts for 130 calls /
366,625 tokens / 959,715 micro-USD. No new uncertain call or unused in-flight
reservation remains. These totals reconcile exactly to the artifact's preparation
and parent execution records, excluding diagnostic child totals.

Across all twelve used packs:

| Category | Calls | Tokens | Micro-USD |
| --- | ---: | ---: | ---: |
| Settled provider receipts | 660 | 786,255 | 1,969,369 |
| Retained historical uncertain allowances | 7 | 2,877,824 | 2,450,208 |
| Used/reserved | **667** | **3,664,079** | **4,419,577** |
| Remaining under revised aggregate ceilings | **333** | **6,335,921** | **5,580,423** |

The uncertain allowance is not confirmed spending and has not been refunded.
This is conservative receipt-derived accounting, not an account invoice. No
additional run was launched simply to use the remaining allowance: this campaign
has finished execution, and the next gates are review and calibration decisions.

### Review and verification

The normal blinded review export succeeded, producing **60 packets and 60 blank
ballots** under `/data/tmp/r3-continue5-20260917/run/review/`. All ballots retain
`pending-reviewer`; no model output has been relabeled as a human decision.
Give a reviewer only the `packets/` and `ballots/` folders, not condition mappings,
raw results or automated scores. Follow the
[existing review workflow](README.md#knowledge-adjudication) to collect judgments
and apply a hash-bound batch to a new artifact without modifying this original.

Final read-only assertions verified all 75 observations in planned order, frozen
runtime integrity, sealed/restricted isolation, matching before/after snapshot
hashes, known usage and exact provider-ledger reconciliation. Sixteen focused
receipt/agent tests passed with real container configuration and zero skips.
Boundary checks, `cargo fmt --check` and diff whitespace checks pass. No new
full-workspace test run is claimed; production code was not changed.

The exact owned PostgreSQL container was removed after verifying its ID and owner
label. No disposable evaluation guests remain. Its temporary database is gone;
frozen source packs, artifacts, receipts, journals and review files remain intact.
Existing services, project data and credentials were not changed.

Evidence: immutable pack `/data/tmp/r3-continue5-20260917`, especially
`run/artifact.json`, `run/report.json`, `run/review/`, per-call journals, and
`private/launch-summary.json`. Reconciliation: `/tmp/r3-continue5-final-budget.json`;
checks: `private/final-verification.json`, `/tmp/r3-continue5-verify.mjs`,
`/tmp/r3-continue5-focused.log`, `/tmp/r3-continue5-boundaries.log`.

## Next gates

Follow-up: the [offline coding diagnosis](coding-diagnosis-20260917.md) reproduced
all 45 submitted grader inputs. It found deadline starvation, missing agent-side
tools and 29 code-passing steps rejected by the strict completion protocol, plus
eight genuine code-test failures. Original outcomes and receipts remain unchanged;
no fix or paid rerun has been performed.

1. Collect genuine human judgments for the 60 knowledge answers. The completed
   execution now permits the ordinary review flow; do not bypass it.
2. Diagnose the coding failures and conservative time allocation offline before
   proposing a separately versioned calibration configuration. More calls alone
   do not establish useful task budgets or fix quality.
3. Finish operational calibration and select/freeze the reference baseline before
   requesting separate official held-out approval. The two-family development
   sample does not establish superiority, customer ROI or release readiness.

**R3 remains incomplete.** The authorization change and completed development run
are recorded; official execution and product-quality gates remain open.
