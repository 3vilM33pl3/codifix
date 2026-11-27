# Atrium crypto dependency update

This MIT-licensed source is from `atrium-crypto` 0.1.3 at Atrium commit
`86081bec81124318a96b9e69921e751bcf926ec3`.

The overlay moves the curve and ECDSA dependencies to RustCrypto's 0.14 and
0.17 versions. The release lock resolves `crypto-bigint` at 0.7.6 or newer.
The updated APIs still use the same compressed DID key encoding, fixed-width
signatures and low-S normalization. The default verifier still rejects high-S
or DER signatures. The upstream compatibility option still accepts them.
