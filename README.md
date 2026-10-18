# Codifix

[![CI](https://github.com/3vilM33pl3/codifix/actions/workflows/ci.yml/badge.svg?branch=main&event=push)](https://github.com/3vilM33pl3/codifix/actions/workflows/ci.yml?query=branch%3Amain) [![Nightly](https://github.com/3vilM33pl3/codifix/actions/workflows/nightly.yml/badge.svg?branch=main&event=schedule)](https://github.com/3vilM33pl3/codifix/actions/workflows/nightly.yml?query=branch%3Amain) [![Prerelease](https://img.shields.io/badge/prerelease-v3.0.0--rc.1-blue?style=flat)](https://github.com/3vilM33pl3/codifix/releases/tag/v3.0.0-rc.1) [![Release date](https://img.shields.io/github/release-date-pre/3vilM33pl3/codifix?label=release%20date&style=flat)](https://github.com/3vilM33pl3/codifix/releases/tag/v3.0.0-rc.1) [![Downloads](https://img.shields.io/github/downloads/3vilM33pl3/codifix/total?label=downloads&style=flat)](https://github.com/3vilM33pl3/codifix/releases) [![Docs](https://img.shields.io/website?url=https%3A%2F%2Fcodifix.dev%2Fdocs&label=docs&style=flat)](https://codifix.dev/docs)

[![License](https://img.shields.io/badge/license-AGPL--3.0--or--later%20%2F%20commercial-2563eb?style=flat)](#license) [![Rust 2024](https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white&style=flat)](Cargo.toml) [![Linux](https://img.shields.io/badge/platform-Linux-333333?logo=linux&logoColor=white&style=flat)](https://github.com/3vilM33pl3/codifix/releases/tag/v3.0.0-rc.1) [![macOS](https://img.shields.io/badge/platform-macOS-333333?logo=apple&logoColor=white&style=flat)](https://github.com/3vilM33pl3/codifix/releases/tag/v3.0.0-rc.1) [![Windows](https://img.shields.io/badge/platform-Windows-0078D4?logo=windows&logoColor=white&style=flat)](https://github.com/3vilM33pl3/codifix/releases/tag/v3.0.0-rc.1) [![Stars](https://img.shields.io/github/stars/3vilM33pl3/codifix?label=stars&style=flat)](https://github.com/3vilM33pl3/codifix) [![Forks](https://img.shields.io/github/forks/3vilM33pl3/codifix?label=forks&style=flat)](https://github.com/3vilM33pl3/codifix/forks) [![Open issues](https://img.shields.io/github/issues/3vilM33pl3/codifix?label=open%20issues&style=flat)](https://github.com/3vilM33pl3/codifix/issues) [![Open PRs](https://img.shields.io/github/issues-pr/3vilM33pl3/codifix?label=open%20PRs&style=flat)](https://github.com/3vilM33pl3/codifix/pulls) [![Last commit](https://img.shields.io/github/last-commit/3vilM33pl3/codifix/main?label=last%20commit&style=flat)](https://github.com/3vilM33pl3/codifix/commits/main) [![Citation](https://img.shields.io/badge/citation-CITATION.cff-2563eb?style=flat)](CITATION.cff)

Codifix gives coding agents and developers durable project knowledge: decisions,
fixes, evidence and handoffs that the next session can retrieve with citations.
It captures and curates work, stores it in PostgreSQL with pgvector, and makes it
available through a browser, TUI, CLI and MCP.

Use one local project or route work to separately operated **knowledge areas**.
Configured teams can share reviewed knowledge privately, ask explicit expertise
questions, and synchronize selected knowledge for offline use. These workflows
require their own setup and permissions; a local query does not automatically
search other areas.

[Website](https://codifix.dev) · [Documentation](https://codifix.dev/docs) · [Codifix v3.0.0-rc.1 prerelease](https://github.com/3vilM33pl3/codifix/releases/tag/v3.0.0-rc.1)

[v3.0.0-rc.1](docs/releases/3.0.0-rc.1.md) is available as a prerelease.
This README and the website describe current `main`, which includes fixes newer
than those package bytes. Use the release inventory for the exact package
commit. Mac and Windows packages are unsigned; stable 3.0.0 is not released.
Windows packages remain a preview of client/operator functionality, not a
complete native server stack. Use `codifix urls` to inspect the configured
application URLs.

## Start here

### Try it with Docker

The reproducible demo needs only Docker; it starts PostgreSQL with pgvector,
the service, and the web UI. Clone the repository and start the stack:

```bash
git clone https://github.com/3vilM33pl3/codifix
cd codifix
docker compose up
```

In a second terminal, load the demo project and ask one question:

```bash
docker compose exec codifix codifix demo
docker compose exec codifix codifix query --project demo --question "How does reinforcement work?"
```

Open `http://localhost:4040` for the browser UI, or run
`docker compose exec codifix codifix tui` for the terminal UI. Follow the
[Quickstart](https://codifix.dev/docs/quickstart) for the complete
demo path.

### Use it in a project

If you are joining an existing company installation, follow
[Team setup](https://codifix.dev/docs/team-setup) and use the approved company
origin and device enrollment. For a new standalone installation, prerequisites,
and verification, start with the
[Install guide](https://codifix.dev/docs/install) or download the
[v3.0.0-rc.1 prerelease](https://github.com/3vilM33pl3/codifix/releases/tag/v3.0.0-rc.1).
Upgrading an existing installation? Read the
[Update guide](https://codifix.dev/docs/install/update) first: the v3 transition
requires a breaking reinstall, not an in-place upgrade.
The setup wizard configures the machine once and the project you are working
in:

```bash
codifix wizard --global
cd /path/to/your-project
codifix wizard --dry-run
codifix wizard
codifix doctor
```

Native installations need a PostgreSQL database with pgvector; the install
guide covers local, hosted, Windows, and package-specific paths. The Windows
x86_64 MSI is per-user, installs under
`%LOCALAPPDATA%\Programs\Codifix`, and adds its `bin` directory to the
user `PATH`.

### Work with it every day

Capture finished work, query before making a change, resume after an
interruption, and inspect or recover when something looks wrong. The
[Daily workflow](https://codifix.dev/docs/daily-workflow) gives the
commands and the [TUI](https://codifix.dev/docs/tui) and
[Web UI](https://codifix.dev/docs/web-ui) make the stored evidence
easy to inspect. Before writing, verify the destination using
[Areas and projects](https://codifix.dev/docs/areas). The
[Jobs reference](https://codifix.dev/docs/reference/cli/jobs) explains why
embedding or consolidation can still be pending after curation succeeds.

For configured teams, [private sharing and expertise](https://codifix.dev/docs/federation)
and [offline synchronization](https://codifix.dev/docs/offline) explain what is
shared, what remains local, and how to inspect freshness and pending work.

### Connect agents

- [Codex Desktop plugin](https://codifix.dev/docs/codex-plugin) — the
  supported MCP connection and desktop workflow skill.
- [Agents](https://codifix.dev/docs/agents) — project setup and
  agent-facing workflows.
- [MCP](https://codifix.dev/docs/mcp) — project-memory retrieval and optional
  explicitly authorized expertise/control tools over stdio or Streamable HTTP.

Use one Codifix MCP connection per client. The Codex plugin guide
explains how to avoid duplicate tool registration and how to verify the active
project.

## What it provides

- Cited answers from lexical, semantic, relation, and code-graph retrieval.
- Project memories with provenance, curation, review proposals, and durable
  re-entry briefings.
- Local TUI and browser UI for memories, activity, review, watchers, and
  runtime health.
- Knowledge-area routing, reviewed private publications, and explicitly
  authorized expertise exchange.
- Selective offline knowledge, company project/device onboarding, and durable
  background jobs with inspection and confirmed retries.
- Coding-agent integration through the CLI, repo-local skills, and MCP.
- Repeatable evaluation with paired ablations, immutable artifacts, gates, and
  cost/latency reporting.

## Evidence and evaluation

Evaluation records measure different things; they are not a single release
score. The [15 October capture-quality pilot](docs/developer/evaluation-runs/2026-10-15-jev-capture-quality-pilot.md)
records a synthetic offline rules baseline and transport/replay tests. It made
no live Jev calls and establishes no provider quality or cost advantage.

The [6 July memory-quality reference](docs/developer/evaluation-runs/2026-07-06-memory-quality-v1-comparison.md)
is a historical local run with a red adversarial-stale gate.
The [3 May Docker benchmark](docs/developer/evaluation-runs/2026-05-03-memory-improvement-v1-full.md)
is separate historical evidence from a different suite. Neither result proves
current product-wide coding improvement.

The [R3 campaign status](evals/r3/README.md) records its own frozen inputs,
isolation requirements and remaining evidence. Fixture tests and CI recovery
are not a new paid campaign or ROI result.

For methodology and commands, see the
[evaluation guide](https://codifix.dev/docs/evals) and
[CLI reference](https://codifix.dev/docs/reference/cli/eval).

## Documentation

The public site is organised with an essentials-first path and optional deep
dives:

- [Quickstart](https://codifix.dev/docs/quickstart)
- [Install](https://codifix.dev/docs/install)
- [Areas and projects](https://codifix.dev/docs/areas)
- [Team setup](https://codifix.dev/docs/team-setup)
- [Daily workflow](https://codifix.dev/docs/daily-workflow)
- [Federation and expertise](https://codifix.dev/docs/federation)
- [Offline and synchronization](https://codifix.dev/docs/offline)
- [Help](https://codifix.dev/docs/help)
- [Operations](https://codifix.dev/docs/operations)
- [How it works](https://codifix.dev/docs/how-it-works)
- [CLI reference](https://codifix.dev/docs/reference/cli)

The [`docs-site/`](docs-site/README.md) directory contains the public site;
the [`docs/`](docs/developer/README.md) tree contains the detailed in-repository
manual and developer reference.

## Development and contributing

Start with [Contributing](CONTRIBUTING.md), the
[developer documentation](docs/developer/README.md), and the
[dev-stack guide](docs/developer/dev-stack.md). The development stack is
isolated from packaged installations; its setup and verification steps belong
in that guide rather than this README.

## License

Codifix is dual-licensed:

- **Open source:** GNU Affero General Public License v3.0 or later; see
  [LICENSE](LICENSE).
- **Commercial:** available under a separate commercial license; see
  [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md).

Contributions are accepted under the repository's open-source license unless
explicitly agreed otherwise in writing. See [Contributing](CONTRIBUTING.md).
