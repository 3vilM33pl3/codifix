# Ash incident lookup — R19

This fictional lookup accepts one field-qualified term such as `service:api`.
The supported fields are service and code. Match only the selected field after
ASCII lowercase normalization. In each selected field, tokens are maximal runs
of ASCII letters or digits; punctuation separates tokens. Match complete tokens,
not substrings. Thus punctuation is a boundary, not part of an unbounded query.
An unknown field or a term containing multiple tokens is an explicit invalid
request, not a fallback to all-fields search.

Restrict authorized project and eligible active records before ranking/limits.
Results retain source/version attribution. Matching a term does not establish
that a diagnosis is correct. This contract is intentionally small; it does not
define full Unicode language processing, fuzzy matching or retrieval tuning.
