# Release response approval

Stop the mismatched install; preserve the manifest, downloaded digest and alias
inspection. Correct the alias and purge the affected delivery cache, then download
again and verify bytes against the authorized stable manifest and platform.
Do not overwrite the signed manifest with the observed digest to silence the
alert. Do not treat key rotation alone as a correction of the wrong object.

Investigate alias-change authority and recipient exposure separately. The present
packet has no forged signature, stolen-key evidence or reproducible-build result.
A current k-red signature would fail signer policy even if cryptographically
valid. Maintain historical receipts without granting retired keys new authority.

Before claiming recovery, confirm the corrected download and verify installation
under revision 7 with a bounded compatibility check. No such completed recovery
or company-wide recipient count is recorded yet.
