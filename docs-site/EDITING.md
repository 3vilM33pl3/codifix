# Editing the docs website

The website text lives in `content/docs/**/*.mdx`.

Open `Codifix Docs.code-workspace` in VSCode when you want to edit prose. It puts the content files first and hides generated folders such as `.next`, `.source`, and `node_modules`.

## Route map

Each MDX file maps to a page route:

| File | Route |
| --- | --- |
| `content/docs/index.mdx` | `/docs` |
| `content/docs/automations.mdx` | `/docs/automations` |
| `content/docs/install/linux-debian.mdx` | `/docs/install/linux-debian` |
| `content/docs/how-it-works/reinforcement.mdx` | `/docs/how-it-works/reinforcement` |

The first page in a folder is normally `index.mdx`.

## Sidebar order

Sidebar labels and ordering are controlled by nearby `meta.json` files:

| File | Controls |
| --- | --- |
| `content/docs/meta.json` | top-level docs navigation |
| `content/docs/install/meta.json` | install section |
| `content/docs/how-it-works/meta.json` | how-it-works section |
| `content/docs/tui/meta.json` | TUI section |
| `content/docs/reference/meta.json` | reference section |
| `content/docs/reference/cli/meta.json` | CLI reference section |
| `content/docs/help/meta.json` | help section |

Single-page topics (`agents.mdx`, `watchers.mdx`, `mcp.mdx`, `evals.mdx`, `operations.mdx`, `automations.mdx`, `web-ui.mdx`) live directly under `content/docs/` and are ordered by the top-level `meta.json`.

When you add a page, add the new file slug to the matching `pages` list.

## Page format

Pages use frontmatter followed by normal Markdown:

```mdx
---
title: "Page title"
description: "Short page description."
---

# Page title

Body text.
```

MDX components are available, but use plain Markdown for normal text. Keep diagrams as fenced `mermaid` blocks and images under `public/images/`.

Detailed workflow guides are canonical Markdown in `../docs/user/`. Their site
routes contain only frontmatter and one Fumadocs `<include>` wrapper. This
includes TUI, Web UI, plugin, daily workflow, areas, team setup/administration,
Mac onboarding, offline synchronization, federation and the exposed CLI guides.
The mapping is enforced by `scripts/check-content.mjs`; update it when adding
a shared route. Never copy the guide body into a wrapper.

Included Markdown uses absolute `https://codifix.dev/docs/...` links so the same
source works on GitHub and the site. Keep images under `public/images/` and use
`https://codifix.dev/images/...` in canonical Markdown; the MDX transform resolves
them locally at build time. Links and images from included content are checked.

The Essentials path stays first. Deep folders use `collapsible: true` and
`defaultOpen: false`; do not expose operator theory ahead of the daily workflow.

## Version and claim boundaries

Describe current `main` and distinguish it from a published package using its
release inventory/source commit. Equal version strings do not establish equal
bytes. Preserve original Memory Layer release notes and historical identifiers.
Do not turn a synthetic test, rehearsal, unsigned package or opt-in capability
into a broad production-readiness claim. Use generic identities and sanitized
examples in public guides.

## Common edits

- Change website text: edit `content/docs/**/*.mdx`.
- Change sidebar order: edit the nearest `meta.json`.
- Add an image: put it in `public/images/` and reference `/images/name.png`.
- Change layout or styling: edit `app/`, `components/`, or `app/global.css`.
- Update a shared interface guide: edit `../docs/user/tui/`, `../docs/user/web-ui.md`, or `../docs/user/codex-desktop-plugin.md`.

## Validate

Run these from `docs-site/`:

```bash
npm run check:content
npm run lint:links
npm run check:assets
npm run test:checks
npm run build
npm run check:built-links
python3 ../scripts/check-codex-plugin.py
```

The rendered-link check runs after a build. It checks actual page/heading IDs,
including included Markdown, so section links remain valid when a shared guide
is shortened or reorganized.
