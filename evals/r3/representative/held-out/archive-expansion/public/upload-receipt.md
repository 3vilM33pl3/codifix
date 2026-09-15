# Import u-53 receipt

The complete compressed upload is 262,144 bytes (256 KiB). Metadata declares
two entries totaling 2 MiB: log-a.txt and log-b.txt, each declared 1 MiB.
The partner signature verifies against an enrolled sender. Its signature covers
the uploaded bytes, not a guarantee that entry metadata matches actual expansion.

Worker w7 checked compressed size and declared totals, then began decoding into
a shared visible import directory. It has no per-entry streaming counter. The
decoder output meter for log-a reached 9 MiB before the operator stopped it.
log-b was not decoded. No completed activation receipt exists.

The signature verifier and compressed-size check both passed. Those successes
did not enforce A5's actual expanded-byte and staging visibility rules. The
receipt does not establish whether the sender intended excessive expansion.
