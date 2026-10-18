# Private federation

Codifix federation connects explicitly enrolled instances through a private
gateway. Ordinary project queries stay local. Sharing requires reviewed
publication or an explicitly confirmed expertise question; joining a space does
not export project knowledge automatically.

These guides describe the current `main` implementation. The repository version
is `3.0.0-rc.1`; that version label and disposable integration evidence do not
establish customer readiness or activation on an installed service. Federation
and expertise remain operator-configured, opt-in capabilities. Check the version
and configuration of every participant before enabling a workflow.

## Choose a workflow

| Goal | Guide |
| --- | --- |
| Set up people, projects and devices | [Team setup](https://codifix.dev/docs/team-setup) |
| Understand instance and knowledge boundaries | [Areas](https://codifix.dev/docs/areas) |
| Review a publication, correction or retained citation | [Sharing workflow](https://codifix.dev/docs/federation/workflow) |
| Ask a private question against an approved collection | [Expertise exchange](https://codifix.dev/docs/federation/expertise) |
| Configure the gateway and participants | [Runtime configuration](https://codifix.dev/docs/federation/runtime) |
| Review remote membership as the separate authority | [Enrollment](https://codifix.dev/docs/federation/enrollment) |
| Use a synchronized laptop copy | [Offline knowledge](https://codifix.dev/docs/offline) |

Start in **More → federation** in the [Web UI](https://codifix.dev/docs/web-ui),
or use the [federation CLI](https://codifix.dev/docs/reference/cli/federation).
Controls require the corresponding configured runtime, current local permissions
and explicit space grants. Local administration and remote space authority are
separate responsibilities; see [administration](https://codifix.dev/docs/team-setup/administration).

## Understand the observations

Queued means a local intent exists. Accepted means the gateway accepted it.
Published means the gateway has verified publication evidence. Replicated means
one named receiver has verified and stored it. None means all peers are current.

Retained shared publications can be inspected offline with current local access.
Fresh expertise answers require origin authorization, even for an already
completed result. Offline search over a downloaded projection is a separate
workflow. Revocation prevents later authorized access but cannot recall delivered
plaintext, screenshots or backups. Signatures establish attribution and integrity,
not factual correctness.

Public Bluesky publication, native Spaces interoperability, arbitrary remote
tools and autonomous cross-instance delegation are not supplied by these guides.
