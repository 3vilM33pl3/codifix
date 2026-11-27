import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

function htmlFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name);
    return entry.isDirectory() ? htmlFiles(file) : entry.name.endsWith('.html') ? [file] : [];
  });
}

export function checkBuiltLinks(output = path.resolve('.next/server/app')) {
  // Validate the rendered IDs, including headings from canonical Markdown and
  // MDX components, rather than trying to reproduce the renderer's slug rules.
  if (!existsSync(path.join(output, 'docs'))) {
    return {
      checked: 0,
      pageCount: 0,
      errors: ['Build the documentation before checking rendered links.'],
    };
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

  return {
    checked,
    pageCount: pages.size,
    errors: [...errors],
  };
}

const entrypoint = process.argv[1] ? pathToFileURL(process.argv[1]).href : null;
if (entrypoint === import.meta.url) {
  const result = checkBuiltLinks();
  if (result.errors.length > 0) {
    console.error(result.errors.join('\n'));
    process.exit(1);
  }
  console.log(`checked ${result.checked} rendered docs links across ${result.pageCount} pages`);
}
