# Separate-authority membership enrollment

`memory-federation-enroll` grants selected instances four private-space capabilities:
`Pull`, `Publish`, `AskExpertise`, and `AnswerExpertise`. It does not turn Workshop
or Homelab into the space authority, grant `ManageMembership`, publish arbitrary
records, create credentials, access a database, or invoke a model/Pi agent.

This is one operator step, not a complete deployment tool. Read the
[runtime prerequisites](federation-runtime.md) and
[local expertise controls](f1-internal-expertise.md) first.

## Prerequisites

- A separately provisioned private gateway/PDS and an explicitly selected
  **separate authority instance ID**. The authority cannot appear among the
  selected members. Provisioning and credential issuance are separate reviewed
  operations; do not use the disposable test administrator against real data.
- The gateway already authenticates the authority and each selected instance.
  Each member uses its own credential. The complete `expertise_fleet` inventory
  declares schema v3 for every configured gateway credential, including the
  authority. Membership records do not enroll a new network credential.
- An HTTPS gateway origin, with normal certificate validation. The operator has
  no cleartext/insecure mode, redirects, or inherited HTTP proxy support.
- Independently distributed gateway receipt public keys. Do not accept a trust
  key learned from the same untrusted response being verified. Receipt keys are
  distinct from the PDS repository signing key.
- An existing authority bearer-token file and JSON config file, both regular,
  owned by the invoking user, mode `0600`, and without hard links. Their immediate
  directories must be owned by that user with mode `0700`. Paths must be absolute
  and contain no symlink components; ancestors must be root/user-owned and not
  group/world-writable (trusted sticky ancestors such as `/tmp` are allowed).
  Enrollment pins directory/file descriptors and rejects changed entries while
  reading; it fails closed on platforms without these Unix guarantees.
  The token is never a CLI argument or printed in output.

Build the operator without changing a running service:

```sh
cargo build --locked -p codifix-federation-runtime --bin codifix-federation-enroll
```

## Prepare and review

Create an operator-owned configuration matching this structure. Values below are
illustrative, not deployment identities or a usable key. Allocate each UUID once
and retain it for exact retry. Use an explicit current review window of at most
24 hours. Select only the members intended to receive all four capabilities;
the operator supports at most eight members per plan.

```json
{
  "schema_version": 1,
  "gateway": "https://company-gateway.example.invalid",
  "gateway_ca_file": null,
  "gateway_ca_sha256": null,
  "token_file": "/private/company/authority.token",
  "receipt_keys": [{
    "generation": 1,
    "first_sequence": 1,
    "public_key": "REPLACE_WITH_INDEPENDENTLY_VERIFIED_RECEIPT_PUBLIC_KEY"
  }],
  "plan": {
    "operation_id": "00000000-0000-4000-8000-000000000001",
    "space": "00000000-0000-4000-8000-000000000002",
    "authority": "00000000-0000-4000-8000-000000000003",
    "author": "company-enrollment-operator",
    "authored_at": "2026-09-19T15:00:00Z",
    "expires_at": "2026-09-19T16:00:00Z",
    "expected_sequence": 0,
    "expected_tip": null,
    "members": [{
      "member": "00000000-0000-4000-8000-000000000004",
      "pull_record": "00000000-0000-4000-8000-000000000005",
      "publish_record": "00000000-0000-4000-8000-000000000006",
      "ask_record": "00000000-0000-4000-8000-000000000007",
      "answer_record": "00000000-0000-4000-8000-000000000008"
    }]
  }
}
```

For an internal CA, set `gateway_ca_file` to the absolute PEM file and
`gateway_ca_sha256` to `{"algorithm":"sha256","hex":"<64 lowercase hex>"}`.
The file bytes must match that reviewed hash. This adds a trusted CA; it does not
disable certificate/hostname verification. The free-form `author` identifies
operator-supplied authorship metadata, not separately authenticated human identity;
the gateway authenticates the authority instance credential.

The default command is an **offline preview**. It does not read the bearer token
or contact the gateway. It validates the config and optional CA, then prints the
exact schema-v3 records, selected members, trust keys, review window and canonical
`review_sha256`. The digest binds the entire typed config, including token path,
endpoint, keys, record IDs and reviewed baseline. JSON whitespace is irrelevant.

```sh
target/debug/memory-federation-enroll /private/company/enrollment.json
```

Before final review, inspect the authenticated, signed published state:

```sh
target/debug/memory-federation-enroll inspect /private/company/enrollment.json
```

Inspection is read-only. Copy `published_through` and `published_tip` into the
plan's `expected_sequence` and `expected_tip`, then produce and review a **new
preview digest**. An empty stream uses `0` and `null`. History is bounded to 4,096
records/16 MiB. Larger spaces require a separately reviewed operator extension.

The operator must run from an explicitly permitted private gateway network; do
not publish the gateway port merely to make inspection work. Current diagnostics
distinguish transport unavailability and authorization denial from page decoding,
binding, policy, signature, receipt, chain, ordering, complete-history and replay
failures. None of these messages establish publication. Check the private TLS
certificate's validity window before treating a transport failure as repository
or receipt corruption.

## Execute and reconcile

After reviewing every exact record, submit that config with the exact lowercase
64-character digest printed by its preview:

```sh
target/debug/memory-federation-enroll execute /private/company/enrollment.json EXACT_REVIEW_SHA256
```

Before its first write, execution verifies nonce-bound signed pages, each audit
receipt, the complete receipt chain, reducer replay and the reviewed baseline.
Additional published records are allowed only when they are exact records from
this same plan, permitting safe partial retries. An unrelated published change
requires a new review. Each write also checks that the review has not expired.

The gateway's `/accept` response is HTTPS-authenticated and checked against the
exact submitted record, but is **not itself a signed publication receipt**.
The operator distinguishes it from the separately verified signed publication
history:

| Result | Meaning | Exit |
| --- | --- | --- |
| `published_proven` | Every planned record appears in the signed published history | 0 |
| `accepted_publication_pending` | Gateway accepted the records; some/all publication proofs are not yet available | 2 |
| Error or timeout | Rejected or unconfirmed; earlier records may already have been accepted | 1 |

This is not a multi-record transaction. A failure does not roll back remote
effects. Preserve the original config and IDs. Inspect after a failure or pending
result; an exact execute retry within the review window reuses those IDs and
receives duplicates for records already accepted. Never generate replacement
IDs to resolve a lost response. Inspection remains available after expiry;
expired execution is refused. An unrelated concurrent publication or expired
review requires operator reconciliation and a newly reviewed plan, not an
automatic workaround. Commands time out after 120 seconds; provider/PDS work may
still continue independently after timeout.

`published_proven` means the pinned gateway attests publication and the accepted
record history is consistent. This operator does not independently download and
verify the PDS repository/CAR. It does not establish customer readiness, backup
recovery or knowledge correctness.

## Finish local authorization separately

Remote membership alone neither exports source knowledge nor enables answering.
Each Memory instance still needs exact-project local grants, a separately
configured internal answer identity, reviewed immutable collections, explicit
requester allowlists and output-destination consent. Follow the
[F1 internal expertise workflow](f1-internal-expertise.md). Leave Homelab Pi and
paid model execution disabled until those separate controls are approved.

Synthetic operator tests exercise dry preview, strict inputs, digest/expiry
refusal, signature rejection, pending-versus-published results, exact retry and
changed-history refusal. They do not provision or activate a live company space.
