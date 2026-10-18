# Company administration

Company administrators connect verified human identities, exact projects and
device approvals. Start in **Access** and check which instance owns the change.
Project administration, company administration, Git access and model disclosure
are separate authorities.

This guide describes current source on `main`, including opt-in company
workflows. Use the selected package's inventory and the running service's
capabilities to establish its exact contract. The
[Mac onboarding guide](https://codifix.dev/docs/mac-onboarding)
records native candidate limits; deployment and provider RBAC must be validated
for the installation you operate.

## Users, projects and devices

Access includes Users, Projects, Devices and Company alongside memberships and
service tokens. Company administration requires global `system_admin`; a project
administrator is not automatically a company administrator. An enrolled OIDC
human can inspect their own Devices/Agent access without administration inventory.
Browser mutations require CSRF protection and a writable service.

- Register users with the configured OIDC issuer and a verified stable subject,
  then assign project memberships. Email alone never identifies or merges an
  account. Effective access includes direct and configured group grants.
- Disabling a Codifix user fences sessions and enrolled devices; it does not
  disable their external identity-provider account.
- Projects have append-only reviewed manifests. An edit binds its current
  revision; a conflicting edit requires fresh review. Pinned tool metadata does
  not install software or authorize disclosure.
- Devices can be revoked independently. New enrollment requires an actual OIDC
  human browser session, not an installation's local-owner token.

## Coordinate an owner grant

Local controls administer the current instance. The Company view coordinates
grants through explicitly configured owner delegates. Choose the enrolled owner,
enter the exact project UUID and verified user subject, review the role and
revisions, then confirm. Owners enforce their own exact-project ceilings and
reject stale revisions or another issuer. Catalog entries do not create access
in another area's database. Trust remains remote expertise only.

An interrupted grant retains its operation ID. Retry the exact request or
reconcile its durable operation; reconciliation checks the owner before retrying
an absent effect. History is not current access. The separately timestamped owner
result shows direct and group roles: removing a direct grant does not remove
group access. Delegate credentials and transport addresses remain server-side.

## Enable selective device delivery

Device delivery uses separate delegates from membership management. Each
non-Trust owner explicitly configures `company_delivery_delegations` with company,
owner, area, shared issuer, dedicated service principal/token, policy revision
and an exact project allowlist. The delivery-delegate provisioner issues its
configured credential once after protected audit commit; it cannot recover a
lost secret or restore revoked authority. Normal roles do not include the custom
`project_scoped_delivery` permission.

The company registry opts in with `delivery_token_env` and
`delivery_policy_revision` for each owner. Keep these separate from membership
management's `token_env`. Both credentials remain server-side. Existing
registries without the delivery pair retain legacy behavior rather than silently
enabling laptop delivery. Trust cannot opt into offline delivery.

Device approval reviews exact owner/area/project scope, manifest hashes, expiry,
offline-copy consent and tools/output destinations. Curated contribution consent
and assistant disclosure each need their own review. Current human permissions
continue limiting an enrolled device; increased human access does not silently
expand it. Revocation stops new transfers but cannot recall offline copies.

## Optional Authentik management

The server-only adapter is disabled by default. An operator may configure
`auth.authentik_management` with an HTTPS provider origin, `token_env`, restricted
`user_path` and optional `invitation_flow` UUID. Give the provider service account
only the necessary user/invitation object permissions. Live Authentik RBAC has
not been rehearsed by this development work; adapter checks do not replace
provider-side restrictions.

Create, invite and provider-disable actions retain durable operation references.
An invitation is created in Authentik but its email is not sent automatically;
use provider invitation administration for delivery. Provider IDs are not
assumed to be OIDC subjects.

For pending or uncertain operations, use the displayed inventory and provider
correlation to reconcile in Authentik before starting another operation. Refresh
reads status and never retries an external action. Credentials and provider
response bodies are excluded from displays.

See [team setup](https://codifix.dev/docs/team-setup) for the device user's
journey and [areas and projects](https://codifix.dev/docs/areas) for ownership.
