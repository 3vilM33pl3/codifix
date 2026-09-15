# M1 financial and pitch appendix

**Status:** M1 preparation only · hypotheses and fictional examples · 2026-09-15

This appendix supports the [federation-first commercial roadmap](README.md) and
the [M1 buyer/pilot scorecard](m1-buyer-pilot-scorecard.md). It is a reusable
working template for customer discovery, cost-model rebuilding and a later deck.
It is not a forecast, price list, offer, partner claim, or evidence of revenue,
ROI, renewal, deployment, or customer readiness. A `HYPOTHESIS` is a proposed assumption to test. Every `EXAMPLE` is deliberately
fictional and must never be relabelled as an observation.
Unknown customer values are `PENDING`, never zero.

## M1 boundary and operating assumptions

M1 prepares the language, inputs and measurement design. M2–M3 rebuild separate
London and New York models and validate pricing; M4 produces the full deck; M5
covers founder-led discovery and later customer work. M1 does not authorize
spending, hiring, deployment on existing services, paid model calls, customer
agreements, or prospect outreach. The matching internal Notion summary is part
of the approved M1 delivery.

The product hypothesis is **Codifix**, a knowledge and governance network for
application and platform/operations enterprise business units. Each unit
operates a locally governed vertex with its own agents, policies, memory and
administrators, while integrating existing agent runtimes, repositories and
operational tools. A customer may run two or more independently administered
instances; shared knowledge is selected, reviewed and attributable.
The first buyer hypothesis is CIO/CTO or IT operations. Application and platform
leads are proposed champions. M1 prioritizes the application/platform business-unit
boundary; MSP, acquired-entity and customer/supplier examples are later discovery
alternatives, not equivalent first-pilot selections.

ATProto is an adapter for publication, repository identity and synchronization;
it does not supply Memory's application authorization or historical authority.
Publication/discovery is therefore a different flow from private gateway Q&A:
the former carries reviewed records through the selected ATProto profile, while
the latter addresses a bounded question and answer between authorized parties
behind the private gateway. Neither flow implies public disclosure, factual
truth, peer consensus, automatic failover or recall of delivered bytes. See
[ADR 0011](../../developer/adr/0011-atproto-federation-adapter.md) and the
[F1 execution evidence](f1-execution.md).

## Reusable assumptions register

Copy this table into a model or interview worksheet. The owner records the
collection method and date; a source can be a customer interview, timed
observation, invoice, infrastructure meter, pilot log, or repository evidence.

| Assumption / input | Value | Unit | Source / collection method | Date | Owner role | Status |
| --- | ---: | --- | --- | --- | --- | --- |
| Units in first pilot | PENDING | business units | Interview CIO/CTO; confirm admin boundary | PENDING | Founder / discovery | OPEN |
| Independently administered vertices | PENDING | vertices | Confirm identity, policy and database ownership | PENDING | Platform lead | OPEN |
| Incident handovers per month | PENDING | incidents/month | Sample incident register for 90 days | PENDING | IT ops lead | OPEN |
| Expert interruptions per incident | PENDING | interruptions/incident | Calendar or on-call log plus interview | PENDING | Engineering manager | OPEN |
| Minutes per interruption | PENDING | minutes | Time sample; separate active work from waiting | PENDING | Engineering manager | OPEN |
| Rework after handover | PENDING | minutes or hours/incident | Compare first accepted answer with corrected outcome | PENDING | Service owner | OPEN |
| Correction reuse rate | PENDING | % of eligible later cases | Trace explicit shared-context retrieval or local corrected-source review across later incidents | PENDING | Knowledge owner | OPEN |
| Onboarding effort per vertex | PENDING | person-hours | Time installation, policy, identity/key and runbook steps | PENDING | Platform lead | OPEN |
| Existing runtime integrations | PENDING | systems | Inventory agent/runtime, repository and ticket interfaces | PENDING | Application lead | OPEN |
| Annual platform fee | PENDING | currency/year | Pricing interview and model sensitivity | PENDING | Founder / finance | HYPOTHESIS |
| Included deployments | PENDING | deployments/year | Packaging test; record overage candidates | PENDING | Founder / finance | HYPOTHESIS |
| Onboarding fee | PENDING | currency/engagement | Estimate from timed runbook, then validate | PENDING | Delivery lead | HYPOTHESIS |
| Usage allowance | PENDING | questions/transfers or tokens | Set a transparent allowance; meter in pilot | PENDING | Platform lead | HYPOTHESIS |
| Compute and database cost | PENDING | currency/month | Provider invoices and measured workloads | PENDING | Platform lead | OPEN |
| Deployment packaging and runbooks | PENDING | person-hours and currency/deployment | Time a clean deployment; record sample N and invoices | PENDING | Delivery lead | OPEN |
| Identity and key lifecycle | PENDING | person-hours and currency/year | Time issuance, custody, rotation and recovery; record N | PENDING | Security lead | OPEN |
| Gateway/PDS hosting and isolation | PENDING | currency/month and person-hours | Meter hosting, isolation and operational duty; record N | PENDING | SRE/platform | OPEN |
| Replica storage and retention | PENDING | GB-month and currency/month | Measure local copies, indexes, retention and backup footprint | PENDING | Platform lead | OPEN |
| Transfer/publication/verification | PENDING | GB/month and currency/month | Meter polling, egress, publication and verification | PENDING | Platform lead | OPEN |
| Recovery and backup | PENDING | person-hours and currency/restore | Time restore drills and record backup/recovery invoices; N | PENDING | SRE/platform | OPEN |
| Security review effort | PENDING | person-hours | Threat model, control test and customer questionnaire | PENDING | Security lead | OPEN |
| Multi-party support load | PENDING | tickets or hours/month | Support log split by vertex and incident | PENDING | Support owner | OPEN |
| Recovery objective | PENDING | RPO/RTO | Customer requirement interview and restore exercise | PENDING | SRE/platform | OPEN |
| Pilot start and revenue start | PENDING | date | Agreement and acceptance criteria | PENDING | Founder | OPEN |
| Inference/provider usage | PENDING | calls, tokens or currency/month | Meter permitted calls and provider invoices; separate fake-provider tests | PENDING | Platform lead | OPEN |
| Curation/review effort | PENDING | person-hours/collection or incident | Time sampled source selection, review and approval; record sample N | PENDING | Knowledge owner | OPEN |
| Administration/policy effort | PENDING | person-hours/month | Time access, grant, key and policy changes; record sample N | PENDING | IT ops lead | OPEN |
| Baseline human effort sample | PENDING | cases and person-hours | Paired baseline/governed sample; record N and inclusion rules | PENDING | Service owner | OPEN |
| Customer loaded rates by role | PENDING | currency/person-hour | Customer finance evidence for each disjoint activity role | PENDING | Buyer finance partner | OPEN |
| Codifix loaded delivery/support rates | PENDING | currency/person-hour | Internal employment/contractor cost basis, separate from customer rates | PENDING | Codifix finance owner | OPEN |
| Currency, period and FX convention | PENDING | currency, dates and conversion rate/date | Agree observation period; source FX only if conversion is needed | PENDING | Finance owner | OPEN |
| Baseline and assisted active person-hours by role | PENDING | hours/matched case | Separate case work, expert interruption and rework; time both conditions | PENDING | Analyst | OPEN |
| Realized reduction in customer spending | PENDING | currency/period | Document changed invoices, contract commitments or approved spending decisions | PENDING | Buyer finance partner | OPEN |
| Incremental customer cash spend | PENDING | currency/month | Invoices, contractor statements and purchase orders | PENDING | Finance owner | OPEN |

Status vocabulary: `HYPOTHESIS` is a testable commercial assumption; `OPEN` is
an uncollected input; `OBSERVED` is measured in a named pilot or fixture;
`VERIFIED` has independent evidence; `REJECTED` is retained with its reason.
Repository tests and disposable F1 evidence can establish software behavior,
but cannot fill customer value, price, support, or willingness-to-pay fields.

## Customer value and delivery cost model

These are measurement/model definitions, not validated customer economics or
revenue-recognition advice. Use one stated currency, matched workload and period
throughout. Keep an eight-week pilot separate from annual extrapolations. Unknown
inputs remain PENDING; zero is allowed only when supported by evidence or clearly
labelled fictional data.

### Customer value formulas

Use **active person-hours**, not elapsed handover or waiting time. Partition
case effort by role into mutually exclusive investigation/action, expert
interruption and rework categories. Sum equivalent matched cases in each arm:

```text
released_case_hours_by_role = baseline_active_case_hours_by_role
                           - assisted_active_case_hours_by_role
valued_case_capacity = sum(released_case_hours_by_role * loaded_rate_by_role)
realized_cash_savings = separately_evidenced_spending_reductions_or_avoided_spend
```

Case effort includes failed investigations, retries requiring human work and
negative changes. Interruption/rework savings are breakdowns of the total, not
additional benefits to add again. Reconcile overlapping logs before calculation;
if they cannot be reconciled, report the value estimate as PENDING.

A loaded rate multiplied by saved hours is **valued capacity**, not realized
cash. An invoice, contract change or approved spending decision must establish
cash savings. If the same contractor hours generate both valued capacity and
reduced invoices, count their benefit only once in the economic view. Waiting-time
improvements stay operational metrics unless the buyer supplies a separate,
attributable economic mechanism with no overlap.

### Customer costs and returns

```text
customer_incremental_cash_cost = product_fees_for_period
                              + customer_provider_and_infrastructure_spend
                              + other_incremental_customer_cash_spend
customer_additional_effort_value = additional_customer_internal_hours_by_role
                                  * corresponding_loaded_rates
customer_total_economic_cost = customer_incremental_cash_cost
                            + sum(customer_additional_effort_value)
economic_benefit = valued_case_capacity_not_already_counted_as_cash
                 + realized_cash_savings
net_economic_value = economic_benefit - customer_total_economic_cost
net_cash_impact = realized_cash_savings - customer_incremental_cash_cost
economic_roi = net_economic_value / customer_total_economic_cost
cash_roi = net_cash_impact / customer_incremental_cash_cost
```

Product fees include the period's platform, onboarding/implementation and usage
charges. Define their allocation to the observation period explicitly; do not
insert annual ARR into an eight-week cost denominator. Count customer-paid
inference once; if included in Codifix's fee, it is not another customer invoice.
Additional curation, review, onboarding and administration belong in customer
cost only when not already included in assisted case effort. Do not also value
contractor time already counted as a cash invoice.

Report economic and cash views separately. A zero denominator produces N/A; a
missing input produces PENDING. Negative results stay negative. No payback claim
follows from staff capacity alone; cash payback requires a sourced cash timeline
whose cumulative net benefit actually covers the upfront outlay.

### Codifix delivery and recurring costs

Keep these vendor costs separate from the buyer's costs and benefits. For each
line record actual cash outlay and additional valued labor separately, without
counting salaries/contractor invoices again as extra labor value.

```text
one_time_delivery_economic_cost = deployment_and_integration
                               + initial_identity_key_and_policy_setup
                               + initial_collection_preparation_and_training
                               + one_time_security_and_recovery_preparation
recurring_economic_cost = compute_database_and_gateway_pds
                       + replica_storage_and_transfer
                       + inference_including_retries_and_uncertain_charges
                       + ongoing_curation_review_and_administration
                       + identity_key_rotation_and_recovery_backup_operations
                       + recurring_security_review_and_multi_party_support
                       + allocated_shared_platform_overhead
pilot_contribution = pilot_fees_for_period
                   - attributable_one_time_and_recurring_vendor_costs
```

Populate both cash and economic cost views from the register. Allocate shared
infrastructure once using a documented driver such as measured workload or
vertices; do not separately allocate its full cost to every unit. Keep one-time
work out of recurring cost unless amortization is explicitly shown in M2–M3.

Correction reuse requires explicit retrieval of the approved shared version, or
origin-local curation and fresh collection review. A retrieved correction is
not an economic benefit until correct use on an eligible later case has been
assessed; the review effort is part of the cost.

### Fictional tabletop convention

This example is deliberately not a customer result. Suppose one incident falls
from 30 to 20 active handover hours: 10 hours × GBP60 = GBP600 of recovered
capacity. Suppose recurring curation adds 12 hours × GBP60 = GBP720 of customer
economic cost, counted once, alongside a GBP1,000 product fee and GBP100 of
incremental customer cash. Total economic cost is GBP1,820 and net economic
value is GBP600 − GBP1,820 = **−GBP1,220**. With no evidenced spend reduction,
realized cash saving is GBP0; cash impact is GBP0 − GBP1,100 paid cash =
**−GBP1,100**. This example has no positive payback claim and illustrates why
valued capacity, economic cost and realized cash must remain separate.

Cost-driver inputs are the register rows above. M2–M3 must populate a `PENDING`
value/unit, method and owner for deployment packaging/runbooks; identity/key
custody and recovery; gateway/PDS hosting and isolation; replica storage;
transfer/publication/verification and egress; inference/provider calls;
curation/review; administration/policy; restore and encrypted offsite recovery;
security review; and multi-party support.

## Packaging hypothesis

Test a simple annual package, with all numbers left `PENDING` until M2–M3:

```text
annual_contract_cash = annual_platform_fee
                     + onboarding_fee
                     + usage_overage
recurring_contract_arr_hypothesis = annual_fixed_subscription
                                  + annual_committed_recurring_usage_minimum
```

The ARR convention above is a modelling hypothesis for M2: exclude one-time
onboarding and uncommitted variable overage; report usage and cash collections
separately. The first-year contract may include onboarding; later renewals do not
automatically repeat that fee. This is not an accounting recognition policy.

The annual platform fee covers the governed control plane, support baseline and
an agreed number of vertices. A deployment allowance covers a stated number of
new or upgraded deployments each year. Onboarding covers discovery, boundary
design, identity/key setup, integration, training and acceptance evidence.
Usage may meter a clearly defined unit such as private questions, reviewed
transfers or provider tokens, with an included allowance and transparent
overage. Replicated or synchronized copies of already admitted knowledge are
not rebilled as new knowledge; price the operational transfer/storage service
once through the package or an explicit infrastructure allowance.

Stress the package against delayed pilots, higher support, extra security
review, recovery requirements and multiple administrators. Revenue begins only
after the applicable federation and customer gates are met. Keep London and New
York scenarios in separate models; do not transplant usage, ACV, retention or
currency assumptions between them.

## One-page narrative: Agentic Engineering → Agentic OS → Agentic Enterprise

**Agentic Engineering → Agentic OS → Agentic Enterprise.** The founder’s thesis
is that useful agents need context and a clear operating environment at each
scale. [Omarchy](https://omarchy.org/) supplies the Agentic OS reference analogy;
Codifix takes the next step into organizational knowledge and authority. This
is a positioning analogy, not a partnership or technical dependency.

**Pain hypothesis.** Application and platform/operations teams can lose time when an incident
crosses an authority boundary: the expert is interrupted, context is repeated,
and a correction may not reach the next eligible case. Existing agent runtimes
can search locally, but they do not by themselves establish which reviewed
context another business unit may disclose or reuse.

**Mechanism.** Codifix is a locally governed knowledge and governance network.
Each business unit keeps its own memory, agents, administrators and policies.
An exact, consented question can receive bounded, cited context through the
private gateway; humans approve and apply the action. Explicit shared-context
retrieval or a newly reviewed local correction preserves provenance. ATProto can
adapt reviewed publication and discovery at the boundary; it is not the private
Q&A authority.

**Proposed first proof.** The storyboard specifies synthetic Application and
Platform vertices, an incident handover, origin approval, later eligible reuse
and recovery/revocation checks; M1 has not run this incident demonstration.
Existing E/F1 evidence separately records disposable bounded Q&A, cited results,
independent identities, policy checks and controlled PDS/isolation exercises.
Those existing demonstrations are not production or customer evidence.

**Economics hypothesis.** Measure paired incidents with sample `N`, baseline and
governed hours, interruptions, rework, correction reuse and onboarding effort.
Report valued capacity separately from realized cash. Subscription ARR excludes
one-time onboarding; economic ROI and cash ROI require different evidence, so no
payback claim is made until a customer demonstrates avoided cash or spend.

**Expansion.** After evidence from an application/platform pilot, the proposed network
could connect acquired entities, suppliers and additional operations teams while
retaining local authority. Packaging may combine annual platform subscription,
included deployments, onboarding and metered usage without billing synchronized
copies as new knowledge.

**Current evidence and gates.** F1 is an internal-use, disposable capability;
customer readiness, production deployment, offsite customer recovery, paid model
use and F2 remain future gates. M1 prepares the scorecard and assumptions; M2–M3
rebuild the financial models and validate pricing; M4 builds the deck; M5 tests
the offer with customers. Omarchy is an analogy for an agentic OS experience,
not a partner, dependency or commitment.

## Two-unit incident-handover demo storyboard

Use synthetic data and two independently administered vertices: **Unit A /
Platform** and **Unit B / Application**. The incident is a fictional
identity-key rotation failure after an application deployment. Humans apply the
approved action; no agent remotely executes work in the other unit.

| Beat | On screen / action | Evidence and guardrail |
| --- | --- | --- |
| 1. Local incident | Unit B records symptoms, scope and current owner; its agent searches only local reviewed memory | Show source versions, author and policy scope; no raw session or path export |
| 2. Need expertise | Unit B prepares an exact question to Unit A: “Which rotation sequence was reviewed for this deployment class?” | Show destination, capability, collection revision and permitted output before consent |
| 3. Governed answer | Unit A’s agent returns a short cited procedure and uncertainty note through the private gateway | Show independent identities, citations, current reviewed source and request receipt; no conversation history or unrestricted query |
| 4. Human action | Unit B's responsible human assesses and authorizes/applies the procedure under existing operational controls | Show human ownership and case/action timestamps; no cross-unit tool execution or invented product approval API |
| 5. Origin-reviewed correction | Unit B identifies a changed prerequisite and proposes a correction to the published knowledge; Unit A's owner reviews and approves a new shared version | Preserve attribution and old evidence; shared approval leaves ordinary local source unchanged |
| 6. Later eligible reuse | A distinct later Unit B case explicitly retrieves/cites the corrected shared version. If using F1 again, Unit A first curates its local correction and renews the collection review | Show relevant later case, version and assessor result; record human review cost; no automatic learning |
| 7. Recovery / boundary | Interrupt the gateway connection and observe recovery within expiry, or explicit expiry; then revoke the relevant grant and verify denied fresh access | Rehearse only supported states; a retained copy is not recallable, and new remote answers require reachability/current authorization |
| 8. Close | Compare baseline and assisted active effort, time, quality and total cost | Synthetic fixture output stays labelled synthetic; any invented numbers remain EXAMPLE; valued capacity and evidenced cash are separate |

The demo distinguishes ATProto publication/discovery from private Q&A verbally and
in the UI: a reviewed publication can be represented by the adapter and later
discovered under its profile; the incident question is addressed through the
private gateway and its Memory policy. Publication does not authorize Q&A, and a
Q&A answer does not publish private evidence.

## Claim-to-evidence register

| Classification | Pitch claim | Evidence available now | Boundary / next evidence |
| --- | --- | --- | --- |
| Controlled demonstration | Separate instances exchange knowledge and bounded expertise in controlled tests | [E integration](2026-09-09-e-completion.md) uses separate service processes/DBs/credentials; [F1](f1-execution.md) uses independent DBs and real clients in a disposable internal rehearsal | Controlled host/test evidence, not customer organizations or proven customer SSO |
| Implemented capability | Answers can be cited and attributable | F1 tests cover reviewed excerpts, author, instance, source version and collection revision | Attribution and integrity do not prove factual truth; cite only the current reviewed source and measure usefulness and correction rate |
| Implemented capability | Sharing is policy controlled and private | Gateway, collection consent, fresh origin checks, isolation and denial tests | Customer threat model, security review and production deployment gates remain open |
| Implemented capability | ATProto is a viable federation adapter | [ADR 0011](../../developer/adr/0011-atproto-federation-adapter.md) records semantic-fit spike and adapter decision | Protocol choice is not production readiness, public publication or native Spaces support |
| Controlled demonstration | Interrupted work has recoverable state | F1 evidence covers leases, retries, expiry, restart and uncertain provider handling | Customer RPO/RTO, offsite recovery and operational runbook evidence remain open |
| Hypothesis | Correction reuse reduces repeated work | Product rule supports explicit shared-context retrieval or independently reviewed local correction | No measured customer value yet; collect eligible later cases with paired baseline/governed incidents |
| Implemented capability | Local MCP and client interfaces provide bounded agent integration paths | [F1 interfaces](f1-execution.md) document Web/CLI/TUI/MCP and operator-bound caller/output consent | Validate named runtime compatibility and effort; no universal-runtime claim |
| Hypothesis | Annual platform plus deployment/onboarding/usage packaging is workable | Packaging hypothesis in this appendix | Pricing, willingness to pay, margin, support load and overage behavior are all `PENDING` |
| Hypothesis | The two-unit incident handover is a useful first buyer demonstration | Storyboard above is a proposed synthetic rehearsal, not a run performed by M1 | Buyer response and measured outcomes require M5 evidence |
| Future work | Durable delegated enterprise work is a future expansion | [F2 remains unchecked](README.md#f-conditional-expansion-after-reliable-knowledge-federation); F1 is bounded, tool-free Q&A | No delegated execution, recursive agent network or complete Agentic Enterprise operation is demonstrated |

Current evidence also has explicit limits: [R3](../../../evals/r3/README.md)
remains incomplete; synthetic/offline harness results do not establish held-out
quality, real-provider latency or ROI. Customer SSO/deployment, full encrypted
offsite recovery and applicable security/release gates remain open. Administrators
and admitted recipients can read plaintext; signatures do not establish truth;
revocation cannot recall delivered bytes. Fresh Q&A needs reachable services and
current authorization. [ADR 0011](../../developer/adr/0011-atproto-federation-adapter.md)
and [F1](f1-execution.md) define these boundaries. The proposed storyboard was not
executed as part of this documentation task.

## M1 handoff

Populate the scorecard before making numeric pitch claims. For every candidate,
capture the two admin boundaries, baseline and governed timings, interruptions,
rework, correction path, onboarding effort and cost drivers. Preserve rejected
claims with their evidence and reason. The later deck should link each material
claim to a row here or mark it `PLANNED`, `HYPOTHESIS` or `PENDING`.
