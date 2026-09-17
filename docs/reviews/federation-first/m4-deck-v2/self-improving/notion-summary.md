## Compact infrastructure edition
**Updated and verified 17 September 2026: self-improving knowledge revision.** Eight core slides plus two appendix slides. Native objects, original URL and sharing are preserved.
[Open the editable compact v2](https://docs.google.com/presentation/d/1dMosQKiMjTx1k9yztYfiURaxQTj_HtOSgrmDYW9Ot_g/edit)
<pdf src="file-upload://3de89907-15f4-817c-b659-00b2c9348ee0"></pdf>
**Codifix is the plumbing and electrical wiring for the Agentic Enterprise. Your platforms keep their interfaces. Codifix does the integration.**
The knowledge and governance network is the foundation. Native-platform adapters connect independently administered business units while each unit retains its own knowledge, agents and authority. Easier, lower-risk AI transformation is an intended benefit awaiting customer evidence.
The original 18-slide deck is preserved unchanged: <mention-page url="https://app.notion.com/p/3de8990715f481448e6dc392c3f853c4"/>. Both editions remain available. There is no fixed fundraising ask.
## Ten-slide narrative
1. Infrastructure for the Agentic Enterprise: plumbing, wiring and the native-interface promise.
2. Great AI platforms, disconnected enterprise: fragmented context and repeated integration/control work.
3. Codifix adapters connect independent units; Memory gateway exchange and the ATProto foundation are distinct.
4. Databricks consumer example, explicitly proposed: native MCP → Codifix controls → permitted expertise → cited answer; human action and origin-reviewed correction.
5. Self-improving knowledge, with human control: prioritize, gather evidence, then reconfirm, flag uncertainty or review a correction for a new version and later reuse. Proactive human notification is visibly planned.
6. Connected context, portable memory and traceable corrections: implemented strengths with supported-interface and evidence limits.
7. Two-team entry, CIO/CTO buyer, application/platform champions and permission-based expansion; £80k pricing hypothesis.
8. Founder shipped work, planned team and the proof points capital should deliver: an end-to-end Databricks connection, a measured qualified pilot and viable delivery economics; no monetary ask.
9. Appendix: architecture, trust limits and Neuralese as future research.
10. Appendix: alternative London/New York funding scenarios and employee AI expense versus retained cash.
## Integration evidence and boundaries
**Databricks is proposed, not implemented or tested end to end.** Its native external MCP route supports Streamable HTTP through a Unity Catalog connection/MCP Service. It requires endpoint reachability, cloud/region availability, connection registration and permissions. Stored Bearer credentials are supported; shared credentials do not establish individual Codifix identity or onward-disclosure consent. Memory still requires an operator-bound output destination and exact-question consent.
No Databricks platform rewrite or ATProto adoption is required by the proposed design. Codifix must still implement and validate the connection. The example consumes Memory expertise; it does not federate arbitrary live Databricks data. Native agent use is Public Preview, and service policies have separate Beta status.
[External MCP registration](https://docs.databricks.com/aws/en/ai-gateway/register-mcp-service) · [Native agent use](https://docs.databricks.com/aws/en/agents/mcp-tools/use-mcp-in-agents) · [HTTP authentication](https://docs.databricks.com/aws/en/query-federation/http#authentication) · [Service policy status](https://docs.databricks.com/aws/en/ai-gateway/govern-mcp-service)
**ATProto supplies identity, signed records and extensible schemas/services. Memory supplies enterprise authorization, reviewed publication history and private expertise Q&A.** Protocol extensibility does not automatically confer arbitrary policy, factual truth or cross-model representation compatibility. Private Q&A uses the addressed Memory gateway mailbox, outside the PDS/publication log/retrieval corpus. The gateway is trusted; authorized recipients can retain readable copies; revocation cannot recall delivered bytes.
[Lexicon](https://atproto.com/specs/lexicon) · [XRPC](https://atproto.com/specs/xrpc) · [DID](https://atproto.com/specs/did) · [Signed repositories](https://atproto.com/specs/repository)
**Neuralese is future research.** Learned or latent representations need compatibility, quality/cost evaluation and an auditable human-review design. No latent-payload implementation, token saving or automatic interoperability is claimed. Native Spaces adoption and broader delegated execution remain future work; upstream Spaces is alpha.
[Translating Neuralese — primary research](https://aclanthology.org/P17-1022/) · [Spaces alpha and limits](https://atproto.com/blog/atproto-spaces-alpha)
## Self-improving knowledge and differentiators
**Codifix seeks supporting evidence and preserves reviewable history as knowledge changes.** The maintenance mechanism is implemented and configurable. Improved quality remains to be measured.

Usage-derived activation identifies frequently used knowledge. Higher source volatility can bring eligible rechecks forward. Here, volatility means recorded provenance-verification status changes; it does not detect every changing enterprise fact. Activation thresholds, review flags, cooldowns, due time and budgets still govern eligibility.

Automatic validation is off by default, with dry-run on and automatic rewording off. Opt-in live validation gathers bounded recorded source snippets, provenance, one-hop related memories, prior validation evidence and source-path repository history. Optional manual/API hybrid fallback scans a bounded tracked repository. This is not unrestricted web research or mathematical proof. Valid evidence references do not prove a model's conclusion.

The three outcomes are distinct:
- **Reconfirm:** clear, supported knowledge refreshes validation metadata without a new content version or required human review.
- **Flag uncertainty:** ambiguous, unsupported or low-confidence findings leave content unchanged. Flag-only findings appear in review-status/query views, not the default pending correction queue.
- **Review a correction:** supported proposals can be applied or rejected by an authorized reviewer. Approval creates an immutable version with retained canonical identity/history; later reuse can be measured.

Neither uncertainty flags nor queued proposals currently sends an external notification. The deck labels **Planned outreach — Next: notify the responsible human.** Ownership routing, channel authorization, acknowledgement and audit design remain future work. Weak evidence does not authorize a rewrite. Explicitly enabled high-confidence wording changes and separate curation replacement policies have different rules; not every memory change requires human approval.

**Connected retrieval** combines meaning, keywords and extracted code relationships. Populated embeddings and an extracted code graph are prerequisites. Graph connections explain retrieval; they are not standalone answer citations. Current channels run sequentially.

**Portable memory** separates canonical knowledge/history from derived vectors and supports different agent clients through Memory interfaces. A new embedding space must be populated and checked before use; already populated supported spaces can be selected without recomputing existing vectors. Multi-backend writes add cost. Embedding support is not universal chat-model or platform compatibility.

**Traceable corrections** retain sources, review outcomes and immutable versions. This combination describes Codifix's focus without asserting competitor exclusivity or proven autonomous improvement.

Independent implementation review: **17 September 2026, 14:17:57 BST, commit `5827aeb`**. Relevant source files were unchanged from the planning audit. Tests were inspected, not rerun for this documentation revision.
- `docs/developer/architecture/memory-reinforcement.md`; `crates/mem-config/src/pipeline.rs:537`: configuration/defaults.
- `crates/mem-reinforce/src/selection.rs:35`; `crates/mem-service/src/repository/handlers/reinforcement.rs:216`: eligibility and volatility.
- `crates/mem-reinforce/src/validate/evidence.rs:71`, `verdict.rs:131`, `apply.rs:80,97,134`, `mod.rs:350,396,573`: evidence, decisions, flags and correction application.
- `docs/user/cli/query.md:41`; `crates/mem-search/src/lib.rs:377`: combined retrieval.
- `docs/user/cli/embeddings.md`; `crates/mem-search/src/embedding_backend.rs:37`, `rebuild.rs:113`: spaces and canonical content.
- `docs/user/cli/review.md`; `crates/mem-curate/src/repository.rs:570`: review and separate curation policy.

Four narrative cases pass the source/policy review: supported knowledge, insufficient evidence, supported correction, and high volatility with activation below threshold. The last case starts no automatic evidence/model call. In dry-run, would-be actions are recorded instead of live content/status changes. Full source register, case definitions and measurement denominators are in `docs/reviews/federation-first/m4-deck-v2/self-improving/evidence-and-handoff.md`.

## Dated product evidence
Repository evidence re-read at `54195f4` on 17 September 2026; concurrent uncommitted work excluded. The durable dossier is `docs/reviews/federation-first/m4-deck-v2/` in `/home/olivier/Projects/memory`. Public links to unpublished commits are not used as evidence.
- `docs/developer/architecture/mcp-server.md`: implemented stdio/Streamable HTTP MCP, scoped credentials and per-request authorization.
- `docs/developer/adr/0011-atproto-federation-adapter.md`: ATProto adapter behind Memory-owned domain/governance responsibilities.
- `docs/reviews/federation-first/f1-execution.md` and `2026-09-09-e-completion.md`: controlled synthetic federation/private-Q&A evidence, independent services/databases on one host; fake provider synthesis in F1. Not customer readiness or measured quality improvement.
- `evals/r3/development-continuation-20260917.md` and `coding-diagnosis-20260917.md`: 75 executions completed; **60 knowledge answers await review; 0/15 complete coding sequences succeeded**. Coding repeats one family across five conditions and three repeats. Harness validity, official evidence and quality did not pass; failures do not isolate Memory's contribution.
- `docs/reviews/four-area-network/README.md`: bounded export/import and metadata-only supervisor work; no real-data area import, integrated agent execution, release activation or live four-area demo.
- `docs/future-development/federation-protocol-spike.md`: Neuralese is an application-representation research direction, not the authorization or federation transport.
Founder credibility is the shipped Rust Memory Layer, code-graph retrieval, MCP integration and evaluation tooling. No hiring pipeline, customer traction, partner commitment, certification or investor endorsement is implied. Tom's briefing remains pre-meeting research; no conversation feedback is claimed.
## Commercial and financial preparation
<mention-page url="https://app.notion.com/p/3dc8990715f4815a812bf2a9262ef282"/> supplies the buyer and pilot preparation. The £80,000 annual package includes two independently administered deployments and remains an unvalidated pricing anchor. Additional deployments, onboarding and usage are separate hypotheses; copied knowledge is not billed again. Base has no pilot/onboarding/overage receipts.
Pilot qualification needs owners on both sides, independent administration, sponsor/budget access, approved sharing, an observable baseline and technical readiness. The eight-week example is two baseline weeks, four assisted weeks and two analysis weeks. Match comparable incidents, retain failures/refusals and agree thresholds before success. Released capacity is distinct from cash savings.
<mention-page url="https://app.notion.com/p/3dc8990715f4813fb281d4a8175c3d1a"/> is the financial source of truth. Both workbooks were independently checked read-only on **17 September 2026, 12:37:56–12:37:59 UTC**: Base, `M2-2026-09-15-v3-assurance`, fingerprint `fnv1a32:d74d6a28`. All 12,354 formulas per location, 371 captured inputs per location, 25 Base outputs per location and 23 reconciliations per location pass; all 304 shared inputs agree. Original captures remain **15 September at 20:38:21.652Z London / 20:38:16.635Z New York**; no new refresh or model change occurred.
[London workbook](https://docs.google.com/spreadsheets/d/1PIUTGbZ2a-7sXsxAl6wbChJVTeKN8Q2K2rv1LRcBbO0/edit) · [New York workbook](https://docs.google.com/spreadsheets/d/1fT9Ta-IjX65NqwcRXFRJq5tW814aqje2Vha9YFfx2es/edit)
<table header-row="true">
<tr>
<td>24-month funding objective</td>
<td>London GBP</td>
<td>New York USD</td>
<td>Workbook range</td>
</tr>
<tr>
<td>Gross funding</td>
<td>£3,226,268.30</td>
<td>\$4,913,192.66</td>
<td>Summary!B23</td>
</tr>
<tr>
<td>Net funding</td>
<td>£3,161,742.93</td>
<td>\$4,814,928.81</td>
<td>Summary!B22</td>
</tr>
<tr>
<td>Expected employee AI expense</td>
<td>£420,000</td>
<td>\$565,992</td>
<td>Summary!B73</td>
</tr>
<tr>
<td>AI headroom retained as cash</td>
<td>£420,000</td>
<td>\$565,992</td>
<td>Summary!B74</td>
</tr>
<tr>
<td>Months 25–27 operating buffer</td>
<td>£504,025.07</td>
<td>\$743,765.21</td>
<td>Summary!B25</td>
</tr>
<tr>
<td>Required month-24 closing cash</td>
<td>£924,025.07</td>
<td>\$1,309,757.21</td>
<td>Summary!B27</td>
</tr>
</table>
New York common GBP gross funding is £3,645,883.54 (`Summary!B32`) at planning FX \$1.3476/£ (`Assumptions!E11`, dated 14 September). These are **alternative company bases for the same workload, never additive**.
Nine planned employees include the founder and an assurance lead; fractional counsel is external. Year-end employees are 8/9/9 (`Summary!C18:E18`). Employee AI allowance is £5,000 monthly at 50% utilisation: expected £2,500 includes £250 subscriptions plus £2,250 metered usage, excluding subscription-included usage from duplicate token charges. Headroom remains cash, not a second expense. The 24-month objective retains AI headroom and months 25–27 operating outflows; it is not a 36-month runway guarantee. Full-36-month AI expense/headroom are each £690,000, separately disclosed.
Full initial/recurring ISO 27001, ISO 42001 and SOC 2 Type II external fees remain unquoted. Existing forecasts contain **no separately costed universal-adapter or Neuralese programme**. Payroll/AI budgets do not establish incremental engineering, compatibility, compute or support coverage. Re-scope and cost these before a funding decision.
## Validation and open handoffs
All ten revised slides were rendered and individually inspected. One consolidated repair made future outreach explicit, connected its branch and aligned inherited text. Final review found no material defects. Native validation confirms 120 editable text frames, 17 connectors/rules, one editable financial table, all ten speaker notes, 10 linked footers, 30 note links and 60 unchanged logo objects. Final revision: `PpuOu-E_XqasQg`. Revised PDF SHA256: `d2e10286c472c115d662b8ac4b86cdd7e39dffed6a5e3358e56bd47fa54de606`. Financial content is unchanged; the model checks above retain their actual earlier timestamps.
Original deck revision `_ojXdF-794TMog`, URL and owner-only sharing remain unchanged. The new native copy is also owner-only in Drive/ChatGPT. No runtime, protocol, model, procurement, hiring or spending changes were made.
**M3 remains open:** actual employee AI spend, engineering throughput/rework, delivery capacity, provider/workload rates, integration maintenance, packaging, legal scope and independent assurance quotes; separately cost expanded connectors and research.
**M5 remains open:** customer discovery, sponsorship, qualified deployment, agreed baseline/thresholds and measured quality, operating value, correction reuse and full delivery economics.

**Additional M3/M5 evidence:** independently adjudicated validation accuracy; missed relevant changes including ineligible/deferred cases; reviewer minutes and queue age reported separately for flags/proposals; input/output/cache tokens and provider charges including failed/retried validation; and approved-correction reuse/recurrence on later eligible incidents. The repository handoff defines denominators, evidence sources, owners and pending thresholds. Existing funding does not establish validation-token volume or proactive-outreach development coverage.

## Preserved pre-revision snapshots
[Dated native compact-v2 backup](https://docs.google.com/presentation/d/1rVYE6pdmJXEi9l9ZGz9UmtG6dJVA3JmUY8BNLpd6p9c/edit), created before editing the existing URL. The original 18-slide presentation remains unchanged.
Previous compact-v2 PDF, before the self-improving revision:
<pdf src="file://%7B%22source%22%3A%22attachment%3A0c22b84b-eeb8-4272-b433-c36cab8ff177%3Acodifix-infrastructure-v2-2026-09-17.pdf%22%2C%22permissionRecord%22%3A%7B%22table%22%3A%22block%22%2C%22id%22%3A%22c561f72b-9ad1-43ca-b1a3-81c386b05172%22%2C%22spaceId%22%3A%22f3b104eb-a126-4998-87e9-7274ac47c8f0%22%7D%7D"></pdf>