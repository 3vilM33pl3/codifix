# M4 native deck content review

Date: **17 September 2026**. **Final technical content sign-off: PASS**, verified
at **12:05:04 UTC** against `edited-final.json`; see the final readback below.
The initial review inspected the prepared
`/tmp/m4-deck-review-20260917/requests/content.json` and native readback
`/tmp/m4-deck-review-20260917/edited-v1.json` against the
[claim register](../m4-claim-register.md) and [live financial evidence](financial-evidence.md).
The v1 findings and their resolution are retained below. This is a text/source
review, not a visual, sharing-permission or financial-table-geometry review.
The builder and native deck were not changed by this reviewer.

## Initial v1 result

All **18 slide IDs, visible text segments and speaker notes match** the prepared
content after whitespace normalization. No material funding/forecast transcription
discrepancy was found. Main-slide claim controls correctly distinguish hypotheses,
controlled implementation evidence, customer proof, planned staffing and financial
preparation. Resolve the notes and source-access items below before final sign-off.

## V1 corrections, resolved in final readback

| Priority | Location | Finding and exact correction |
| --- | --- | --- |
| Required | GitHub sources throughout notes | All nine distinct repository paths exist in local commit `89104dd`, but the tested public GitHub ADR URL returns HTTP 404. The coordinator independently found that commit unavailable publicly and is replacing these links with verified M1/M2 Notion pages and a new M4 evidence dossier. Retain repository path + commit as provenance text, and verify the replacement destination contains the claim evidence. Do not publish the engineering branch as a link repair. |
| Required | Slide 7 notes | The sentence attributes separate OS processes to the F1 fixture as a whole. Replace with: **“F1 uses independent databases and real HTTP service applications/workers in one test process; the accompanying E gate separately starts service executables. Both are controlled synthetic evidence on one host, not customer SSO or multi-host deployment.”** Source: `f1-execution.md`, lines 681–688 at `89104dd`. |
| Required | Slide 7 notes | “R1 adds durable outbox/inbox” conflates workstreams. Replace with: **“C supplies transactional federation outboxes/inboxes and receiver cursors; R1 supplies durable local jobs, leases, fencing and operator-visible failures.”** Source: roadmap C/R1 and their linked completion records. No need to change the visible capability table. |
| Required | Slide 15 notes | “Not placed in public PDS records, public logs/timelines” is too narrow: F1 excludes these payloads from the private PDS too. Replace with: **“Private questions and answers use Memory's addressed gateway mailbox. They never enter the PDS, shared publication log, general project timeline or retrieval corpus.”** Source: `f1-execution.md`, lines 36–42. |
| Required | Slide 15 notes | “Revocation blocks future authorized access” obscures retained offline readability. Replace with: **“Revocation denies subsequent gateway operations; already authorized in-flight work may complete. Previously delivered copies remain readable while local authorization permits, and cannot be recalled.”** Source: `federation-security.md`, lines 112–121. The visible “revocation cannot recall bytes” statement is correct. |
| Required | Slide 16 notes | Add the sample definition immediately after 0/15: **“These are three repeats of one coding repository family under five conditions, not 15 independent tasks.”** The existing no-quality-claim and harness/real-failure distinctions are correct. Source: `evals/r3/development-continuation-20260917.md`, lines 62–69. |
| Clarify | Slide 4 notes | Prefer **“Repository signatures establish authenticity and integrity within the verified scope; they do not prove factual truth, original human authorship or human review.”** to the broad “prove provenance/authenticity.” Memory preserves separate publisher/author/version attribution. Source: ADR 0011, lines 122–126. |
| Clarify | Slide 11 notes | Add: **“The eight-week illustration begins after deployment readiness and charter approval; inadequate case counts do not become success when the calendar ends.”** Current visible gate qualification is already appropriate. Source: `m1-buyer-pilot-scorecard.md`, lines 252–257. |

## Claims checked and retained

- **Founder, slide 13:** “creator of Memory Layer” and the listed built
  capabilities are supportable. `CITATION.cff` names Olivier Van Acker as software
  author; `Cargo.toml` names him at line 41. The frozen README describes Rust,
  code-graph retrieval, MCP and evaluation; the implementation contains the
  graph-assisted query channel and corresponding crates. This supports creator /
  builder positioning, not sole authorship of every dependency or a quality claim.
- **Headcount:** nine planned employees **including founder**, plus external
  fractional counsel, matches the current model. Certification/report completion
  is explicitly denied; assurance and AI-throughput inputs remain hypotheses.
- **Evaluation:** the retained report still has `execution_valid=true`, 60 pending
  knowledge reviews and `harness_valid`, `official_evidence`, `quality_passed=false`.
  No old 18.1%, perfect-recall or 41%-token-saving headline is retained as a new
  product claim. The original 0/15 result is not changed by diagnostic replay.
- **Four-area notes:** the materializer, atomic inactive-target import library and
  pre-dispatch input binding are supported at frozen `89104dd`. No activation or
  release is claimed. The later `0436b17` exporter is not claimed on a slide and
  need not be retroactively added to the frozen notes. Input binding does not
  itself grant collection/source disclosure consent.
- **Financials, slides 10/17/18:** £80,000/two deployments, forecast ARR/revenue,
  NRR arithmetic, regional gross margins, £3.23m/$4.91m gross financing, first-24-
  month AI expense/headroom and operating buffer agree with the live finance
  register at the shown rounding. Alternative company bases are not added;
  retained cash is not expensed again. Full assurance scope remains unquoted.
- **Competition:** positive adjacent-capability descriptions and a Codifix focus
  claim avoid unsupported feature-absence, exclusivity or superiority assertions.
  The exact primary links match those supplied from M4 planning. No duplicate
  external feature audit was performed for this review.

## Register update and follow-through

Added source **C5** to the claim register for the completed 17 September native
finance readback, with links to both evidence files. Slides 10, 13, 17 and 18 now
refer to it. The historical 15 September source-capture timestamps remain intact.

The v1 follow-through called for a fresh native read after corrections and source
replacement. That technical readback is completed below. Destination-content,
visual review and the M4 completion checkpoint remain coordinator-owned.

## Final native readback — passed

Verified **17 September 2026, 12:05:04 UTC** using the updated prepared content and
`/tmp/m4-deck-review-20260917/edited-final.json`. All 18 native slide IDs, visible
text segments and speaker notes match the updated content after whitespace
normalization. The final review retains the frozen technical evidence at
`89104dd` and the claim register's separate `0436b17` follow-up; it does not chase
subsequent engineering changes.

| Reviewed item | Final outcome |
| --- | --- |
| Slide 4 signature scope | Correctly distinguishes verified authenticity/integrity from truth, original human authorship and human review. |
| Slide 5 correction reuse | Retains local curation and renewed collection review, human action and measurement limitations. |
| Slide 7 test topology and durability attribution | Distinguishes F1 service applications/workers in one test process from accompanying E executables; correctly assigns federation outboxes/inboxes to C and local durable jobs to R1. Synthetic/fake-provider and customer-readiness limits remain. |
| Slide 11 pilot clock | Explicitly begins after readiness and charter approval; inadequate case counts cannot pass by elapsed time. |
| Slide 15 private Q&A and revocation | Excludes questions/answers from any PDS, shared publication log, timeline and retrieval corpus. Explains subsequent gateway denial, in-flight completion and locally authorized readability of retained copies. |
| Slide 16 R3 sample | Adds one coding family × five conditions × three repeats; retains the original 0/15 outcome and all nonpassing quality/official/harness flags. |
| Repository-source access | No unavailable frozen GitHub URL remains anywhere in the native JSON, including hyperlink styles. Exact repository paths and `89104dd` remain provenance text. |
| Replacement hyperlinks | Native footer links target the supplied M1/M2 Notion pages, workbooks, official comparator sources or the new M4 dossier. Technical slides 4, 5, 7, 12, 15 and 16 link to the exact M4 dossier below; notes also carry that dossier. |

The replacement technical destination is
[M4 dated evidence dossier](https://app.notion.com/p/3de8990715f481448e6dc392c3f853c4).
This review confirms its exact use in native notes and links; the coordinator
owns verification of destination content and sharing access, alongside visual,
object/financial-table and final M4 completion checks. No new technical content
correction remains from this review.

Input SHA-256 values:

- Updated `requests/content.json`:
  `f5601844ecba30b53806b307483b0fd1be4a2b8867b296bf38edda957c42a97e`.
- Native `edited-final.json`:
  `630037fbf484fe612761feeacb7056995693ddae38b95f5c22ac23eb83aa3e1f`.
