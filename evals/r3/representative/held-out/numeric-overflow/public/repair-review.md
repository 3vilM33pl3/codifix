# O16 repair review — pending

Accumulate header plus both lengths in a wider checked type, reject totals above
65,535 before narrowing/allocation, and pass the verified total to the writer.
Retain a final emitted-byte agreement check. Keep the previous complete bundle
unchanged on validation or write failure; do not claim a partial file is valid.

Test exact maximum, one byte over, zero payloads, individually valid fields with
too-large sum and validator/writer agreement. Check all intermediate additions,
not only the final cast. A future larger wire format requires explicit version
design, not silent truncation of the current one. No rollout or format expansion
has been approved.
