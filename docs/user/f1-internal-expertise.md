# F1: internal expertise exchange

F1 is software for explicit private questions between independently administered
Codifix instances. Its disposable rehearsal is not an installed service,
customer release, R3 evaluation or F2 delegation system. Ordinary queries remain
local. Start with deterministic excerpts; no model account is required.

## Installation checklist for a separately approved activation

1. Provision separate Codifix instances/databases for the participating areas,
   plus a dedicated company-space gateway database and private PDS. Use separate
   runtime credentials. Follow the [gateway topology](https://github.com/3vilM33pl3/codifix/blob/main/deploy/federation/README.md),
   [runtime configuration](https://codifix.dev/docs/federation/runtime) and
   [recovery contract](https://github.com/3vilM33pl3/codifix/blob/main/docs/developer/architecture/federation-security.md).
   Apply all append-only migrations with the schema-owner workflow and then
   reviewed gateway grants. Do not run fixture bootstrap utilities against real
   data. Keep PDS credentials, raw repository routes and gateway SQL inaccessible
   to participants. Expose only fixed gateway API routes behind private TLS.
2. Assign stable space/authority/instance/project UUIDs. Enroll distinct instance
   credentials and independently distribute gateway receipt-key trust. Add every
   credential identity to the gateway's `expertise_fleet` as `["UUID", 3]`.
   F1 fails closed without a complete compatible fleet. Existing v2 history
   remains unchanged; these are unpublished Codifix-profile Lexicons, not native
   Spaces or public Bluesky interoperability.
3. Explicitly grant remote `Pull`, `Publish`, `AskExpertise` and `AnswerExpertise`
   memberships as needed using the authority's existing federation membership
   workflow. Separately grant local capabilities to authorized users. Local
   administrator status does not imply gateway authority. Wait for accepted and
   published membership state; queued is not enrollment.
4. At each answering instance, provision an internal service principal with
   **only that project's** read/export permissions and local `AnswerExpertise`.
   It must not have global/group authority. Set `expertise_principal` in that
   instance's federation JSON. This identity is separate from human requesters.
5. Enable `expertise_browser` only where browser use is intended. In a single-user
   loopback installation, a human-operated CLI/TUI may explicitly use
   `federation --project PROJECT --local-human-session ...`; this exchanges the
   existing local-owner authority for an in-memory browser session with CSRF
   protection. It does not change credentials, permit MCP or authorize onward
   model disclosure. The ordinary static local API token is not an expertise
   caller. For multi-user CLI/TUI service-token use,
   configure `expertise_callers` with the exact local principal UUID, token ID
   (not token secret) and `output_destination: "local_human"`. Use a different
   token binding for MCP, with its actual operator-approved assistant destination.
   Unknown bindings fail closed. Never hand a local-human token to an assistant
   and call that onward disclosure consent.
6. Prepare a collection in Web Federation, CLI `collections draft`, or the
   focused TUI. Select exact curated versions and explicit evidence; none is
   included by default. Review **every disclosed byte**, author, label, digest,
   requester allowlist and output destination before activation. Keep
   `model_destinations` empty and deterministic mode unless separately approved.
   Activation is standing consent for that frozen revision, not a copy of its
   contents to the gateway. Grant the configured internal principal access to
   that collection using its inspected grant controls.
7. Synchronize both instances and inspect capability advertisements. A source
   update, tombstone or withdrawal makes the old material ineligible immediately;
   answering must not silently use the replacement. Prepare and review a new
   collection revision to share changed content.
8. Ask one explicit test question in each direction. Inspect the exact selected
   destination, question, collection revision, model mode, output destination,
   digest and expiry before submission. Read the result by original request ID;
   verify original author, authenticated answering instance, exact source
   version, evidence labels and digests. Test withdrawal and revocation before
   broadening the approved collection.

Configuration examples and exact JSON shapes are in
[runtime configuration](https://codifix.dev/docs/federation/runtime#f1-private-expertise-opt-in).
The capability-detail API is
`GET /v1/projects/{slug}/federation/{space}/capabilities/{publisher}/{record}`;
listing is bounded and may require following a cursor through an empty filtered
page. See [OpenAPI](https://github.com/3vilM33pl3/codifix/blob/main/docs/api/openapi.yaml) and [CLI commands](https://codifix.dev/docs/reference/cli/federation).

## Daily use and recovery

Web provides collection review, service grants, discovery, question confirmation
and results under Federation. `codifix federation --project example-project tui` provides
the focused terminal equivalent: F5 prepares/reads and F6 confirms only the exact
prepared mutation. CLI `collections`, `capabilities`, `ask prepare`, `ask submit`
and `answer` use the same project-scoped API and policy.

For an ambiguous submission, preserve the exact review and request ID and retry
before its expiry. Do not edit a payload under an existing ID. A new request is a
new disclosure and can incur repeat billing. `uncertain` provider execution is
never automatically reinvoked; unavailable or malformed models fail without a
fallback. Offline origins cannot authorize fresh answer delivery, including
previously completed answers. Metadata/status reads never bypass that check.

Workers poll every two seconds, with one active answer per responder, 30-second
leases renewed at ten seconds, a 120-second question deadline and at most 32
outstanding questions per requester. Restart restores recoverable delivery from
PostgreSQL. Expired leases cannot overwrite accepted results. Payloads expire
after 24 hours; metadata/idempotency receipts after 30 days. Reads enforce expiry
even before the minute-based cleanup. Holds preserve storage, not access.

MCP exposes dedicated `codifix_expertise_discover`, `codifix_expertise_prepare`,
`codifix_expertise_ask` and `codifix_expertise_result` tools. Submission requires
`[mcp] read_only = false`, operator-bound onward destination and exact consent.
An assistant cannot self-authorize by supplying a provider name. Answers are
untrusted data, not instructions; no remote tools, recursive requests or work
delegation are executed.

Optional synthesis additionally requires gateway model policy, reviewed collection
consent, requester provider consent and exact responder model configuration.
Positive call/token budgets are mandatory. The default enabled ceiling is one
invocation per question, 800 output tokens and 20 daily calls per responder;
40,000 tokens are conservatively reserved per call without refunding unknown
usage. See the [restricted model configuration](https://codifix.dev/docs/federation/runtime#optional-f1-synthesis).
This documentation authorizes no paid calls.

## Adding another participating instance

Provision its own instance/database and credentials, add its schema-v3 identity
to the complete gateway fleet, then deliberately enroll the required remote and
local capabilities. Existing collections still deny it. Review new revisions
whose requester allowlists explicitly include the new instance, and grant its answering
identity only its own reviewed collections if reverse exchange is wanted. No
code change is needed; a restart/configuration rollout and reviewed policy changes
are required. Do not infer access merely from company-space membership.

## What the rehearsal establishes

The [executable PDS gate](https://github.com/3vilM33pl3/codifix/blob/main/tests/federation-pds/README.md#f1-expertise-gate)
checks two-way answers, real clients, third-instance enrollment, recovery and
fake-model failures using disposable data. The
[TLS isolation gate](https://github.com/3vilM33pl3/codifix/blob/main/tests/federation-isolation/README.md) tests fixed
routes, credentials and cross-space denial. The
[execution evidence](https://github.com/3vilM33pl3/codifix/blob/main/docs/reviews/federation-first/f1-execution.md) records exact
coverage and limitations. F1's separate-database service apps run in one test
process; the accompanying Stage E regression launches separate service processes.
This is not independent physical-host, disaster-recovery or customer evidence.

Infrastructure administrators and authorized recipients can read shared plaintext.
Revocation cannot recall transferred bytes, browser screenshots, backups or
recipients' copies. Gateway signatures prove attribution/integrity, not factual
truth. Scoped Lean reducer proofs do not prove deployed authentication, model
behavior or network liveness. Retention/legal holds, offsite restore, image
security, real identity provisioning and activation of an installation need their own
approved operational exercise.
