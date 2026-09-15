# Dune desktop release policy, revision 7

Release artifacts are identified by channel, platform, version and SHA-256 digest.
A filename or version string alone is not identity. Only a manifest signed by an
enrolled release key can nominate a production artifact. The verifier checks
manifest signature, signer authorization, channel/platform binding and the bytes'
digest before installation. Transport TLS alone does not replace these checks.

Key k-red was authorized through August 8 and retired on August 9. Key k-blue is
authorized for current stable releases. Historical verification can retain old
receipts; that does not authorize k-red to nominate a new current artifact.

Publishing release notes does not publish an artifact. Notes may explain intent
but cannot override the signed manifest or prove binary behavior. A valid artifact
signature establishes release attribution and integrity, not freedom from bugs.
