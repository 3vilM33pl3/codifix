// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';

// Explicit Windows MSI allocation: each later release must compare greater.
// Preserve the already allocated 3.0.0 identities when adding candidates.
export function releaseIdentity(version) {
  const windowsBuild = { '3.0.0-rc.1': 1, '3.0.0': 2, '3.1.0-rc.1': 3, '3.1.0': 4 }[version];
  assert(windowsBuild, 'unallocated Codifix release; expected an explicitly allocated release');
  return { debianVersion: version.replace('-rc.', '~rc.'), windowsBuild };
}
