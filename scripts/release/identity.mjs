// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';

// Explicit Windows MSI allocation: stable must compare greater than its RC.
// Add future candidates here and to the release workflow as a reviewed change.
export function releaseIdentity(version) {
  const windowsBuild = { '3.0.0-rc.1': 1, '3.0.0': 2 }[version];
  assert(windowsBuild, 'unallocated Codifix release; expected 3.0.0-rc.1 or 3.0.0');
  return { debianVersion: version.replace('-rc.', '~rc.'), windowsBuild };
}
