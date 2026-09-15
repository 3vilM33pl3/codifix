# Dune build receipts, August 12

Build b-402: version 4.2.0, linux-x64, candidate channel, source commit c-771;
artifact digest aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.
Build b-403: version 4.2.0, linux-x64, stable channel, source commit c-772;
artifact digest bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.
Both files were named dune-4.2.0-linux.tar. Their digests are synthetic fixture
values, not hashes of executable bytes supplied in this exercise.

The signed stable manifest nominates b-403's digest for linux-x64 version 4.2.0
and verifies under k-blue. Verification receipt v-51 records the exact manifest
and current signer policy revision 7. No signed promotion of b-402 exists.

Unit-test jobs passed for both builds. There is no cross-machine reproducible-build
comparison or complete end-to-end compatibility report in this packet. Commit
c-772 changed packaging metadata; the receipt does not establish runtime behavior.
