import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { checkBuiltLinks } from './check-built-links.mjs';

function run(t, home, guide = '<h1 id="guide">Guide</h1>') {
  const temporary = mkdtempSync(path.join(tmpdir(), 'codifix-rendered-links-'));
  t.after(() => rmSync(temporary, { recursive: true }));
  const output = path.join(temporary, '.next/server/app');
  mkdirSync(path.join(output, 'docs'), { recursive: true });
  writeFileSync(path.join(output, 'docs.html'), home);
  writeFileSync(path.join(output, 'docs/guide.html'), guide);
  return checkBuiltLinks(output);
}

test('rendered links preserve query strings and encoded fragments', (t) => {
  const result = run(t, '<a href="/docs/guide?mode=read&amp;scope=one#r%C3%A9sum%C3%A9">Read</a>', '<h1 id="résumé">Resume</h1>');
  assert.deepEqual(result.errors, []);
  assert.equal(result.checked, 1);
});

test('missing rendered heading fails even when the target page exists', (t) => {
  const result = run(t, '<a href="https://codifix.dev/docs/guide#stale-heading">Read</a>');
  assert.deepEqual(result.errors, ['/docs: missing rendered anchor /docs/guide#stale-heading']);
});

test('missing rendered page fails', (t) => {
  const result = run(t, '<a href="/docs/missing">Read</a>');
  assert.deepEqual(result.errors, ['/docs: missing rendered page /docs/missing']);
});

test('same-page fragments work and external links are not treated as site pages', (t) => {
  const result = run(t, '<h1 id="home">Home</h1><a href="#home">Here</a><a href="https://example.com/docs/other#remote">External</a>');
  assert.deepEqual(result.errors, []);
  assert.equal(result.checked, 1);
});
