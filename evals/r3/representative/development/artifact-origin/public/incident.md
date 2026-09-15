# Download mismatch inspection

At 14:05 UTC the stable download alias returned dune-4.2.0-linux.tar with digest
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa. TLS validation
succeeded. The operator's verifier rejected the bytes because the signed stable
manifest expected digest bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.
No install was started on this inspected host.

The alias cache held an object populated from the candidate channel at 13:48.
The object-store inspection found both build objects intact. This evidence
supports a wrong alias target/cache entry, not a measured SHA-256 collision.
It does not determine whether the initial alias change was accidental or hostile.

One inspected host is recorded. Download counts, verifier versions on other hosts
and recipient inventory are unavailable. The mismatch alert is not a deployment
count, and the lack of an install here is not proof about other machines.
