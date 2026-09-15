# Employee AI funding — evidence and planning assumptions

**Checked:** 15 September 2026 · **Owner:** founder / finance · **Review:** M3

## What the sources establish

- **Vendor-reported usage:** Anthropic reports approximately USD150–250 per
  developer/month and USD13 per active day across enterprise Claude Code
  deployments; 90% of users remain below USD30 per active day. Workload, model
  selection and parallel agents materially affect usage. This is vendor guidance,
  not an independently audited benchmark for Codifix's operating model.
  [Claude Code cost guidance](https://code.claude.com/docs/en/costs)
- **Executive ambition:** In the publisher's interview transcript, Jensen Huang
  gives a thought experiment pairing USD500,000 annual engineer compensation with
  at least USD250,000 in token consumption. This supports considering substantial
  AI investment; it does not establish realised spending, productivity or a
  transferable staffing ratio.
  [All-In Podcast interview transcript](https://www.linkedin.com/videos/allinpod_jensen-huang-if-that-500000-engineer-activity-7440742599600230400-drTE)
- **Founder-selected hypothesis:** The approved M2 revision sets GBP5,000 per
  active employee/month, 50% expected utilisation and an eight-role hiring plan.
  Base spending comprises GBP250 subscriptions plus GBP2,250 incremental metered
  usage. These figures apply to all eight roles and include no demonstrated
  headcount-replacement or output-equivalence claim. They are considerably above
  Anthropic's reported ordinary deployment average.

## Dated API rate reference

USD per million tokens; direct API, standard processing. These are reference
prices for workload measurement, not fixed 36-month forecast rates or a provider
commitment. The financial engine uses the editable all-in employee budget.

| Model | Uncached input | Cache read | Cache write | Output |
| --- | ---: | ---: | ---: | ---: |
| OpenAI GPT-6 Astra, short context | 10.00 | 1.00 | 12.50 | 50.00 |
| OpenAI GPT-5.6 Sol, short context | 4.00 | 0.40 | 5.00 | 20.00 |
| OpenAI GPT-5.6 Terra, short context | 2.00 | 0.20 | 2.50 | 12.00 |
| OpenAI GPT-5.6 Luna, short context | 0.20 | 0.02 | 0.25 | 1.20 |
| Anthropic Claude Opus 5 | 5.00 | 0.50 | 6.25 / 10.00 | 25.00 |
| Anthropic Claude Sonnet 5 | 2.00 | 0.20 | 2.50 / 4.00 | 10.00 |
| Anthropic Claude Sonnet 4.6 | 3.00 | 0.30 | 3.75 / 6.00 | 15.00 |

OpenAI's listed long-context input/cache rates are twice these short-context
rates; output is 1.5 times. Batch/Flex and Fast mode have different schedules;
eligible regional processing adds 10%. Sol's promotion runs at least through
21 November 2026; no undisclosed post-promotion rate is assumed.
[OpenAI API pricing](https://developers.openai.com/api/docs/pricing)

Anthropic cache-write pairs mean five-minute / one-hour retention. These models
retain standard rates across their full context window. Batch, Fast mode,
geography and separately billed tools can alter costs. Sonnet 5's USD2/10
input/output price is now standard, superseding its earlier introductory-price
expiry. [Anthropic API pricing](https://platform.claude.com/docs/en/about-claude/pricing)

## Billing boundary and M3 evidence

API-equivalent usage is not automatically a cash invoice: subscription plans
include usage, and displayed session estimates may use list prices. Reconcile
paid seats, included consumption, incremental credits/API charges and contractual
rates against supplier bills. Do not charge subscription-included tokens again.
[Claude Code billing guidance](https://code.claude.com/docs/en/costs)

Unused allowance remains earmarked cash headroom; it is neither an expense nor a
second supplier payment. Keep employee AI separate from customer-facing Memory
inference, customer-owned runtimes and ordinary software tools.

M3 must capture per-person and per-workflow model/tier, input/cache/output tokens,
parallel-agent activity, failures/retries, actual invoices and FX. Pair spending
with accepted engineering output, elapsed time, review/rework, defect rates and
delivery capacity before validating the smaller team or revising utilisation.
