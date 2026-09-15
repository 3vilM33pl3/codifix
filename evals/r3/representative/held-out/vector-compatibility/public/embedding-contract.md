# Juniper search profile — V8

This fictional search service binds an index generation and each query to an
embedding profile. The profile includes resolved model revision, dimensions,
input normalization revision, query/document prefix rules and distance metric.
Equality of vector length alone does not establish compatibility. A floating
provider alias is not a resolved model revision.

V8 requires exact profile equality for semantic ranking. Every indexed chunk
also binds a source version and chunk digest. Missing profile metadata is
unknown compatibility and must fail closed, not be treated as the current
default. Incompatible requests expose an explicit unavailable-profile outcome.
An operator may select an independently documented lexical mode, but the service
must not silently substitute it and report semantic success.

Prepare a replacement index as a separate generation from a fixed corpus
snapshot. Verify complete eligible-source coverage, source/chunk bindings and
profile homogeneity before switching the active generation. Keep a recoverable
previous generation under its own profile. This contract says nothing about a
particular real embedding vendor's internals or overall answer quality.
