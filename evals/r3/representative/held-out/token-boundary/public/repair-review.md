# R19 repair review — pending

Validate the field and single-token term, restrict authorized project/active
eligibility first, tokenize only the selected field with the declared ASCII
rule, then apply the existing order/limit. Keep exact version/source metadata.
Do not widen retrieval to notes when the requested field has no match.

Test case folding, hyphen/underscore boundaries, interior substrings, other-field
matches, unknown fields, multiple-token terms and cross-project candidates
ahead of valid local hits. A whitespace wrapper is not the declared tokenizer.
No fuzzy/embedding/ranking tuning, live corpus export or rollout is approved.
