# Local permissions and gateway capabilities

B2 is a pure policy contract, not new service routes or an authentication system.
`PublicationState::authorize_pull` and `authorize_publication` intersect three
independent conditions: exact-project effective local permissions, an explicit
local sharing grant, and the current gateway membership capability. Any missing
condition denies access. The functions do not mutate grants or the replica.

| Operation | Required local permissions | Explicit local grant and independent gateway capability |
| --- | --- | --- |
| Live shared read/pull | MemoryRead | Pull |
| Content, evidence, state, relation, proposal or proposal resolution | MemoryRead + MemoryCurate + BundleExport | Publish |
| Tombstone state publication | Above + MemoryDelete | Publish |
| Membership grant/revocation | MemoryRead + AuthManage | ManageMembership, plus publisher is the configured space authority |

These are permission sets, not role comparisons. Current Reader/Writer presets
cannot publish; Operator has the local publication prerequisites but still needs
explicit sharing opt-in and gateway enrollment. Admin does not bypass either.
SystemAdmin alone grants nothing here. AuthManage or even a delegated gateway
ManageMembership capability does not make an instance the space authority.

The local grant binds a server-resolved principal UUID, local project UUID,
instance UUID and space UUID. A grant for another user/project/instance/space
cannot be reused. Global permissions may contribute to the exact project's local
permission set, but cannot manufacture that explicit grant. Nil local identities
fail closed. Read-only service mode denies all publication, including membership.
The operation is derived from the actual record; callers cannot mislabel a
tombstone or membership record to select weaker requirements.

Local context/grants deliberately have no wire deserializer. The service must
resolve credentials and exact project permissions centrally, load the explicit
sharing grant from trusted storage, and bind it to the enrolled instance. These
constructors do not authenticate anyone. Current `AuthenticatedPrincipal` resolves
permissions through `permissions_for_project`; `has_anywhere` must never authorize
a project-scoped federation action. No automatic mapping from local principal IDs,
OIDC subjects, role names or authorship to instance authority is introduced.

The pure check is only for publication intent. When durable integration lands,
recheck current local permissions/grant/review when acting on an intent, then
authenticate and recheck gateway membership and reducer semantics within the
gateway acceptance transaction. An earlier policy success is not a durable
authorization receipt. A revoked gateway capability immediately fails new checks;
only explicit authority re-enrollment can restore it.

Ownership, causation, versions and evidence dependencies remain reducer checks.
Review-byte binding is not proof of human review. Offline-copy retention, legal
holds, model forwarding, credential verification, local grant storage/admin UI and
Web/CLI/TUI routes remain downstream work. No live policy has been enabled by this
module, and no ATProto/runtime/database dependency enters the neutral crate.
