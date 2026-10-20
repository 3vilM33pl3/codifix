// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { releaseIdentity } from './identity.mjs';

const version = process.argv[2];
releaseIdentity(version);
const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--no-deps', '--locked', '--format-version', '1']));
for (const pkg of metadata.packages) assert.equal(pkg.version, version, pkg.name);
for (const dir of ['web', 'docs-site']) {
  assert.equal(JSON.parse(readFileSync(`${dir}/package.json`)).version, version, dir);
  const lock = JSON.parse(readFileSync(`${dir}/package-lock.json`));
  assert.equal(lock.version, version, `${dir} lock`);
  assert.equal(lock.packages[''].version, version, `${dir} root lock`);
}
assert.equal(JSON.parse(readFileSync('plugins/codifix/.codex-plugin/plugin.json')).version, version, 'Codifix plugin');
for (const skill of readdirSync('.agents/skills', { withFileTypes: true }).filter(e => e.isDirectory())) {
  const agentFile = `.agents/skills/${skill.name}/agents/openai.yaml`;
  if (existsSync(agentFile)) {
    assert.match(readFileSync(agentFile, 'utf8'), new RegExp(`^version: ${version.replaceAll('.', '\\.')}\\s*$`, 'm'), agentFile);
  }
  const file = `.agents/skills/${skill.name}/SKILL.md`;
  assert.match(readFileSync(file, 'utf8'), new RegExp(`^version: ${version.replaceAll('.', '\\.')}\\s*$`, 'm'), file);
}
assert.equal(execFileSync('git', ['status', '--porcelain', '--untracked-files=no'], { encoding: 'utf8' }).trim(), '', 'tracked source must be clean');
console.log(`Release contract verified: ${version} at ${execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim()}`);
