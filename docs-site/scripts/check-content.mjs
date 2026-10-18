import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';

import {
  collectDocsContent,
  docsSiteRoot,
  includedSpecifiers,
  repositoryRoot,
} from './content-utils.mjs';

const { records, errors } = collectDocsContent();

const sharedGuidePairs = [
  ['content/docs/tui/index.mdx', 'docs/user/tui/README.md'],
  ...[
    'activity',
    'agents',
    'automations',
    'embeddings',
    'errors',
    'memories',
    'project',
    'query',
    'resume',
    'review',
    'skills',
    'watchers',
  ].map((name) => [`content/docs/tui/${name}.mdx`, `docs/user/tui/${name}.md`]),
  ['content/docs/web-ui.mdx', 'docs/user/web-ui.md'],
  ['content/docs/codex-plugin.mdx', 'docs/user/codex-desktop-plugin.md'],
  ['content/docs/daily-workflow.mdx', 'docs/user/daily-workflow.md'],
  ['content/docs/areas.mdx', 'docs/user/areas-and-projects.md'],
  ['content/docs/team-setup/index.mdx', 'docs/user/team-setup.md'],
  ['content/docs/team-setup/administration.mdx', 'docs/user/company-administration.md'],
  ['content/docs/mac-onboarding.mdx', 'docs/user/mac-onboarding.md'],
  ['content/docs/offline.mdx', 'docs/user/offline-and-sync.md'],
  ['content/docs/how-it-works/area-routing.mdx', 'docs/user/area-routing.md'],
  ['content/docs/help/known-limitations.mdx', 'docs/user/release-compatibility.md'],
  ['content/docs/mcp.mdx', 'docs/user/cli/mcp.md'],
  ['content/docs/federation/index.mdx', 'docs/user/federation.md'],
  ['content/docs/federation/workflow.mdx', 'docs/user/federation-workflow.md'],
  ['content/docs/federation/runtime.mdx', 'docs/user/federation-runtime.md'],
  ['content/docs/federation/enrollment.mdx', 'docs/user/federation-enrollment.md'],
  ['content/docs/federation/expertise.mdx', 'docs/user/f1-internal-expertise.md'],
  ...['jobs', 'projects', 'supervisor', 'federation'].map((name) => [
    `content/docs/reference/cli/${name}.mdx`,
    `docs/user/cli/${name}.md`,
  ]),
];

for (const [siteRelative, canonicalRelative] of sharedGuidePairs) {
  const siteFile = path.join(docsSiteRoot, siteRelative);
  const canonicalFile = path.join(repositoryRoot, canonicalRelative);
  const label = siteRelative.replaceAll(path.sep, '/');

  if (!existsSync(siteFile)) {
    errors.push(`${label}: missing shared-guide wrapper`);
    continue;
  }
  if (!existsSync(canonicalFile)) {
    errors.push(`${label}: missing canonical source ${canonicalRelative}`);
    continue;
  }

  const wrapper = readFileSync(siteFile, 'utf8');
  const includes = includedSpecifiers(wrapper).map((specifier) =>
    path.resolve(path.dirname(siteFile), specifier.split('#', 1)[0]),
  );
  if (!includes.includes(canonicalFile)) {
    errors.push(`${label}: must include canonical source ${canonicalRelative}`);
  }
  const body = wrapper.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, '').trim();
  if (!/^<include>[^<]+<\/include>$/.test(body)) {
    errors.push(`${label}: shared guide must contain only frontmatter and one canonical include`);
  }
}

const workspaceManifest = readFileSync(path.join(repositoryRoot, 'Cargo.toml'), 'utf8');
const workspacePackage = workspaceManifest.match(
  /^\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m,
);

if (!workspacePackage) {
  errors.push('Cargo.toml: could not resolve workspace.package version');
} else {
  const releaseVersion = workspacePackage[1];
  const releaseLabel = `v${releaseVersion}`;
  const isPrerelease = releaseVersion.includes('-');
  const docsPackage = JSON.parse(
    readFileSync(path.join(docsSiteRoot, 'package.json'), 'utf8'),
  );

  if (docsPackage.version !== releaseVersion) {
    errors.push(
      `package.json: version ${docsPackage.version} does not match Cargo workspace ${releaseVersion}`,
    );
  }

  // A source candidate is not a newly published stable distribution. Check its
  // source identity without rewriting the existing stable installation guides.
  const releaseReferences = isPrerelease ? [
    ['README.md', `[${releaseLabel}]`],
    ['CHANGELOG.md', `## ${releaseVersion} -`],
    ['docs/user/release-compatibility.md', `[${releaseLabel}]`],
    ['docs-site/content/docs/index.mdx', `[${releaseVersion} prerelease]`],
  ] : [
    ['README.md', `[${releaseLabel}]`],
    ['CHANGELOG.md', `## ${releaseVersion} -`],
    [
      'docs/user/release-compatibility.md',
      `Codifix ${releaseLabel} is the current stable release.`,
    ],
    ['docs-site/content/docs/index.mdx', `Codifix ${releaseLabel} is available.`],
    [
      'docs/user/release-compatibility.md',
      `Codifix ${releaseLabel} is the current stable release.`,
    ],
    [
      'docs-site/content/docs/install/update.mdx',
      `Codifix ${releaseLabel} is the current stable release.`,
    ],
    ['docs-site/content/docs/install/index.mdx', `[${releaseLabel}]`],
    ['docs-site/lib/layout.shared.tsx', `text: '${releaseLabel}'`],
    ['docs-site/components/demo/demo-data.ts', `version: "${releaseVersion}"`],
  ];

  for (const [relativeFile, expectedText] of releaseReferences) {
    const absoluteFile = path.join(repositoryRoot, relativeFile);
    if (!existsSync(absoluteFile)) {
      errors.push(`${relativeFile}: missing release-aware source`);
      continue;
    }
    if (!readFileSync(absoluteFile, 'utf8').includes(expectedText)) {
      errors.push(
        `${relativeFile}: expected current release marker ${JSON.stringify(expectedText)}`,
      );
    }
  }
}

if (errors.length > 0) {
  console.error(errors.join('\n'));
  process.exit(1);
}

console.log(`checked ${records.length} documentation source file(s) and includes`);
