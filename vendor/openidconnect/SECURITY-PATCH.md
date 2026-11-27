# OpenID Connect RSA dependency update

This MIT-licensed source is from `openidconnect` 4.1.0 at upstream commit
`e20ddbb01ca68356f2979bc125f5613ab45b4e7e`.

The overlay uses RustCrypto RSA 0.10 and its high-level PKCS#1 v1.5 and
digest-sized PSS APIs with the existing algorithms. It preserves JSON Web Key
validation and serialization, RSA byte encoding and fallible RNG injection.
Other signing algorithms, OAuth transport, discovery and token checks are
unchanged.
