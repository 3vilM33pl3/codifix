# Numeric safety evidence limits

The diagnostic demonstrates an aggregate arithmetic bug and a detected writer
size disagreement. It does not establish exploitable memory corruption, data
exfiltration or the number of affected bundles. Unsigned wrapping is stipulated
v9 behavior here, not a claim about every compiler's default overflow behavior.

Saturating a size can avoid wrap while still describing too few bytes; it is
not valid acceptance under O16. A correct arithmetic check does not prove all
allocation, persistence or security behavior. Those need separate tests and
deployment evidence, none of which is supplied here.
