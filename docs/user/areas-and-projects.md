# Areas and projects

An **area** is an independently operated knowledge boundary. A **project** groups
work within that area. Choose the area that owns the knowledge before choosing
the project; a matching project name in another area does not make it the same
project or grant access to it.

| Area | Intended knowledge | Laptop delivery |
| --- | --- | --- |
| Workshop | Software and engineering projects | Eligible reviewed projects |
| Homelab | Infrastructure and operational knowledge | Eligible reviewed projects |
| General | General knowledge outside the other areas | Eligible reviewed projects |
| Trust | Restricted organization knowledge and selected expertise | Remote expertise only |

These are routing boundaries, not automatic classifiers. Operators map exact
repositories and projects to their owning area. Each area has its own instance
identity, configuration and access policy. Trust knowledge is not copied to a
laptop by company onboarding; expertise access needs its own approved workflow.

## Choose your working context

From a configured repository, start `codifix tui`. Its operator-owned routing
selects the matching local area. Outside a mapped repository, select explicitly:

```bash
codifix --area workshop tui --project example-app
codifix --area workshop status --project example-app --json
codifix --area workshop query --project example-app --question "What changed?"
```

Check the area and instance in the TUI header or status JSON before writing.
An unavailable selected area fails; it does not redirect writes to General.
`codifix urls` lists configuration-derived entry points, not verified live health.

Local selection uses explicit `--area` first, then an exact repository route,
then an exact project-only route, then General. Conflicting matches fail. Two
repositories can use the same project slug, so a slug alone is not a global
identity. Company onboarding uses the complete area, owner instance, project UUID
and reviewed manifest revision.

## Access and sharing

Area routing selects a destination; it does not grant permission. Human project
roles, Git repository credentials, device offline-copy consent, curated
contribution consent and model-disclosure consent are separate. A project
assignment alone does not authorize copying knowledge or sending it to an AI
provider.

Curated laptop copies retain owner identity, versions and citations in a separate
read-only projection. Raw captures and session bodies remain local. Revocation
stops new authorized transfers but cannot recall bytes already delivered.

## Continue

- [Set up a team device and project](https://codifix.dev/docs/team-setup).
- [Understand offline copies and synchronization](https://codifix.dev/docs/offline).
- [Configure exact area routes](https://codifix.dev/docs/how-it-works/area-routing).
- [Administer company access](https://codifix.dev/docs/team-setup/administration).

The routing and onboarding details describe current source on `main`. The
published `3.0.0` package is identified by its release inventory. Native onboarding remains separately scoped; see its guide.
