// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const audit = JSON.parse(readFileSync(process.argv[2], 'utf8'));
assert(Array.isArray(audit.vulnerabilities?.list), 'complete cargo-audit result required');
assert.equal(audit.vulnerabilities.count, audit.vulnerabilities.list.length, 'inconsistent audit result');
assert.equal(audit.vulnerabilities.count, 0, 'Rust vulnerabilities block publication');
assert.equal(audit.vulnerabilities.found, false, 'inconsistent audit status');
assert(audit.warnings && typeof audit.warnings === 'object' && !Array.isArray(audit.warnings), 'cargo-audit warnings required');
for (const [kind, findings] of Object.entries(audit.warnings)) {
  assert(Array.isArray(findings), `invalid ${kind} warning list`);
  assert.equal(findings.length, 0, `Rust ${kind} warnings block publication`);
}
