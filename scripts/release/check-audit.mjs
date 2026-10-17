// SPDX-License-Identifier: AGPL-3.0-or-later
// Keep the original audit and its nonzero result visible; never pass new findings.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const audit = JSON.parse(readFileSync(process.argv[2], 'utf8'));
assert(Array.isArray(audit.vulnerabilities?.list), 'complete cargo-audit result required');
for (const finding of audit.vulnerabilities.list) {
  assert.equal(finding.advisory.id, 'RUSTSEC-2023-0071', 'new Rust vulnerability blocks publication');
  assert.equal(finding.package.name, 'rsa');
  assert.equal(finding.package.version, '0.9.10');
  assert.deepEqual(finding.versions.patched, [], 'an available upstream fix requires remediation');
  console.warn('KNOWN UNPATCHED RUSTSEC-2023-0071: Rust audit is NOT clean. Memory OIDC uses public-key verification; see docs/reviews/security-dependencies-20261017.md.');
}
for (const [kind, findings] of Object.entries(audit.warnings ?? {})) {
  if (kind === 'yanked') {
    for (const finding of findings) {
      assert.equal(finding.package.name, 'spin', 'new yanked dependency requires review');
      assert.equal(finding.package.version, '0.9.8');
      console.warn('KNOWN YANKED DEPENDENCY: spin 0.9.8 remains in the dependency graph.');
    }
  } else assert.equal(findings.length, 0, `new ${kind} warning requires review`);
}
