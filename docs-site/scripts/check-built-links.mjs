import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

// Validate the rendered IDs, including headings from canonical Markdown and
// MDX components, rather than trying to reproduce the renderer's slug rules.
const output = path.resolve('.next/server/app');
if (!existsSync(path.join(output, 'docs'))) {
  console.error('Build the documentation before checking rendered links.');
  process.exit(1);
}

function htmlFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name);
    return entry.isDirectory() ? htmlFiles(file) : entry.name.endsWith('.html') ? [file] : [];
  });
}

const pages = new Map();
for (const file of htmlFiles(path.join(output, 'docs'))) {
  const route = `/${path.relative(output, file).replaceAll(path.sep, '/').replace(/\.html$/, '')}`;
  pages.set(route, readFileSync(file, 'utf8'));
}
if (existsSync(path.join(output, 'docs.html'))) {
  pages.set('/docs', readFileSync(path.join(output, 'docs.html'), 'utf8'));
}

const errors = new Set();
let checked = 0;
for (const [route, html] of pages) {
  for (const [, href] of html.matchAll(/<a\b[^>]*\bhref="([^"]+)"/g)) {
    const target = new URL(href.replaceAll('&amp;', '&'), `https://codifix.dev${route}`);
    if (target.origin !== 'https://codifix.dev' || !/^\/docs(?:\/|$)/.test(target.pathname)) continue;
    const pathname = target.pathname.replace(/\/$/, '');
    const destination = pages.get(pathname);
    checked++;
    if (destination === undefined) {
      errors.add(`${route}: missing rendered page ${target.pathname}`);
      continue;
    }
    if (target.hash) {
      const anchor = decodeURIComponent(target.hash.slice(1));
      const ids = new Set([...destination.matchAll(/\bid="([^"]+)"/g)].map((match) => match[1]));
      if (!ids.has(anchor)) errors.add(`${route}: missing rendered anchor ${target.pathname}#${anchor}`);
    }
  }
}

if (errors.size) {
  console.error([...errors].join('\n'));
  process.exit(1);
}
console.log(`checked ${checked} rendered docs links across ${pages.size} pages`);
