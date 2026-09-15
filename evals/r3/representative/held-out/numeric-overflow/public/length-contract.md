# Birch binary bundle sizing — O16

This fictional packer validates a bundle before allocation or writing. On-wire
individual lengths are unsigned 16-bit integers. A bundle has two payloads plus
a 16-byte header. The permitted total encoded size is at most 65,535 bytes.
Compute the full aggregate in a wider checked type, compare it with the limit,
then narrow only after validation. Valid individual fields do not imply a valid
aggregate. Any failed validation leaves output and prior bundle state unchanged.

Both the size validator and writer must use the same checked total contract.
Reject malformed, overflowing or over-limit input explicitly. Do not wrap,
saturate, clamp or silently drop payload bytes to fit the wire field. The contract
describes bounded integer arithmetic, not an actual memory allocator or exploit.
