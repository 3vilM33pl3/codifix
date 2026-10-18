import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const checker = fileURLToPath(new URL('./check-built-links.mjs', import.meta.url));

function run(t, home, guide = '<h1 id="guide">Guide</h1>') {
  const temporary = mkdtempSync(path.join(tmpdir(), 'codifix-rendered-links-'));
  t.after(() => rmSync(temporary, { recursive: true }));
  const output = path.join(temporary, '.next/server/app');
  mkdirSync(path.join(output, 'docs'), { recursive: true });
  writeFileSync(path.join(output, 'docs.html'), home);
  writeFileSync(path.join(output, 'docs/guide.html'), guide);
  return spawnSync(process.execPath, [checker], { cwd: temporary, encoding: 'utf8' });
}

test('rendered links preserve query strings and encoded fragments', (t) => {
  const result = run(t, '<a href="/docs/guide?mode=read&amp;scope=one#r%C3%A9sum%C3%A9">Read</a>', '<h1 id="résumé">Resume</h1>');
  assert.equal(result.status, 0, result.stderr);
});

test('missing rendered heading fails even when the target page exists', (t) => {
  const result = run(t, '<a href="https://codifix.dev/docs/guide#stale-heading">Read</a>');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /missing rendered anchor \/docs\/guide#stale-heading/);
});

test('missing rendered page fails', (t) => {
  const result = run(t, '<a href="/docs/missing">Read</a>');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /missing rendered page \/docs\/missing/);
});

test('same-page fragments work and external links are not treated as site pages', (t) => {
  const result = run(t, '<h1 id="home">Home</h1><a href="#home">Here</a><a href="https://example.com/docs/other#remote">External</a>');
  assert.equal(result.status, 0, result.stderr);
});
