# Self-improving knowledge: evidence and handoff

Independent source review: **17 September 2026, 14:17:57 BST**, commit `5827aeb`. Relevant maintenance, review, retrieval and embedding files were unchanged from the planning audit at `053509e`. Tests below were inspected, not executed for this documentation-only revision. These are implementation and policy checks, not measured quality improvement.

## Claim register extension

| Slide | Claim / status | Source at snapshot | Boundary |
|---|---|---|---|
| 5 | Usage prioritizes rechecking — implemented | `crates/mem-service/src/reinforcement.rs:149`; `crates/mem-reinforce/src/selection.rs:35` | Eligibility uses access-derived activation, not a stored importance score. Popularity does not establish correctness. |
| 5 | Higher volatility advances eligible rechecks — implemented | `crates/mem-service/src/repository/handlers/reinforcement.rs:216`; `crates/mem-reinforce/src/scoring.rs:130` | EWMA of recorded provenance-verification status changes, not every source edit or enterprise fact. Activation, cooldown, review state, due time and budgets still gate checks. |
| 5 | Validation is configurable — implemented, disabled by default | `crates/mem-config/src/pipeline.rs:537` | `validation_enabled=false`, `validation_dry_run=true`, `auto_apply_rewording=false`. Actual customer deployment configuration was not asserted. |
| 5 | Bounded supporting evidence — implemented | `crates/mem-reinforce/src/validate/evidence.rs:71`; `verdict.rs:131`; `mod.rs:291` in that directory | Recorded source snippets, provenance, related memory and repository history. Optional manual/API hybrid fallback is bounded. No unrestricted web search or mathematical proof. Evidence-reference validation does not prove model conclusions. |
| 5 | Uncertainty flag and correction proposal are separate — implemented | `crates/mem-reinforce/src/validate/apply.rs:80,134`; `mod.rs:350,396,573`; `docs/user/cli/review.md` | Weak findings leave content unchanged and flag it. Flags do not enter the default proposal queue. Supported proposals can be applied/rejected by an authorized reviewer. |
| 5 | Reviewed corrections create immutable history — implemented | `crates/mem-reinforce/src/validate/mod.rs:573`; `docs/developer/adr/0002-reinforcement-scores-outside-immutable-versions.md` | Factual-correction path needs authorized approval. Valid high-confidence rewording can auto-apply only if enabled. Separate curation replacement policies differ (`crates/mem-curate/src/repository.rs:570`). Not every change requires human approval. |
| 5 | Notify responsible human — future work | No automatic external contact in flags/proposals; `crates/mem-supervisor/src/proposal.rs` AskHuman is a typed suggestion | Requires ownership routing, authorized channels, acknowledgement, permissions and audit work. No Slack/email message or runtime configuration added. |
| 6 | Connected retrieval — implemented | `crates/mem-search/src/lib.rs:377`; `docs/user/cli/query.md:41` | Meaning, keywords and code relationships. Populated embeddings and extracted code graph are prerequisites; graph relations explain retrieval, not standalone answer citations. Channels run sequentially. |
| 6 | Portable canonical memory — implemented within supported interfaces | `crates/mem-search/src/rebuild.rs:113`; `embedding_backend.rs:37`; `lib.rs:150`; `docs/user/cli/embeddings.md`; `docs/developer/architecture/mcp-server.md` | New embedding spaces must be populated and checked before use. Existing populated supported spaces can be selected without re-embedding. Multi-backend writes incur cost. Embedding support does not equal chat-model support or universal connector coverage. |
| 6 | Traceable corrections — implemented | `docs/user/cli/review.md`; `crates/mem-reinforce/src/validate/mod.rs:573` | Sources, canonical identity, versions and review outcomes remain linked. Authentic signatures do not prove truth or human approval. |
| 5–6 | Self-improvement and distinctive combination — value hypothesis / product focus | Mechanisms above | No proven autonomous quality improvement, competitor-exclusivity claim or universal portability. R3 limitations remain in notes and parent register. |

## Four narrative checks

Cases assume validation enabled and **dry-run disabled**, except the ineligible case. In default dry-run, would-be actions/evidence and cooldown are recorded instead of applying live content/status changes.

| Case | Evidence / policy expectation | Presentation check | Result |
|---|---|---|---|
| Supported knowledge | Valid, clear verdict refreshes validation metadata/cooldown. No new content version or required human review. `apply.rs:97`; inspected fixture `crates/mem-reinforce/tests/db.rs:986`. | Reconfirm branch is separate from correction/version path; details in notes. | Pass |
| Insufficient evidence | Unsupported, ambiguous or low-confidence verdict sets `needs_review`, leaves content unchanged, does not create pending correction or send contact. `apply.rs:80`; `mod.rs:396`. | Flag uncertainty branch is distinct; future notification dashed and marked Next. | Pass |
| Proposed correction | Outdated/partially valid, supported proposal enters pending review. Authorized approval creates immutable version; rejection leaves content unchanged. `apply.rs:134`; `mod.rs:573`. | Review a correction precedes new version and later reuse; notes retain rejection and rewording exceptions. | Pass |
| High volatility but not eligible | Activation below threshold yields no automatic evidence/model call. Review flags/cooldown/budgets can also block checks. `selection.rs:35`; inspected fixture `tests/db.rs:391`. | Priority label does not say volatility alone triggers work; notes explain eligibility. | Pass |

## M3 / M5 measurement handoff — open

M3 owns cost/capacity and M5 supplies customer evidence. The following extend the existing handoff; no thresholds or success results are invented.

| Evidence to collect | Definition and denominator | Required source / owner | Acceptance field |
|---|---|---|---|
| Validation accuracy | Independently adjudicated correct verdicts / all sampled completed validation verdicts; also report wrong reconfirmations and inappropriate proposed changes separately. | Blinded reviewer labels against available source evidence; evaluation owner with originating team. | Customer/evaluation-agreed threshold pending. |
| Missed changes | Relevant changed claims still wrong/unflagged / independently identified relevant changed claims. Include ineligible, budget-deferred and not-yet-due cases to expose coverage and latency. | Independent change sample, verification status/history and scheduling logs; source owner/evaluation lead. | Threshold and acceptable detection delay pending. |
| Human review effort | Reviewer minutes, queue age and apply/reject counts per flag/proposal and per eligible incident. Flags and proposal queues reported separately. | Review logs and time samples; source owner/support lead. | Capacity and response thresholds pending. |
| Validation-token cost | Input/output/cache token quantities and provider charges per attempted/completed validation and per maintained memory. Include failures, retries, fallback and human curation costs separately. | Provider invoices/rates and run logs; M3 cost owner. | Spending/cost-per-useful-result limit pending. |
| Correction reuse | Approved corrections cited/applied successfully in later eligible incidents / eligible later incidents where the correction applies. Also measure recurrence of the corrected error. | Version/citation history, origin review and independent incident outcomes; M5 pilot lead. | Customer-agreed reuse/recurrence thresholds pending. |

Measured accuracy needs sufficient comparable cases and independent review. Implementation and citable references alone do not validate quality, productivity, customer readiness or ROI. Current funding scenarios do not isolate validation-token volume or proactive-outreach development cost; M3 must validate scope and allocation before presenting them as covered.
