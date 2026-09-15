# Matcher v12 inspection

v12 strips any `field:` prefix, lowercases the remaining text and searches it as
a substring of concatenated service, code and notes. It takes the first two hits
before applying the requested project restriction. Unknown fields are silently
treated as ordinary text. Notes include manually entered incident descriptions.

A proposed patch r29 adds spaces around the term in the concatenated text.
Another raises the candidate limit. Neither preserves field scope, the stated
punctuation tokenization or project-before-limit eligibility. Neither is approved.
No changes to the ranking algorithm are authorized by the repair review.
