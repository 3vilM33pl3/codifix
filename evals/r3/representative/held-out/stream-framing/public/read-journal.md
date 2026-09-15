# Selected byte reads

All entries are hexadecimal bytes. Each row is a separate connection unless
explicitly labeled as successive reads. This is synthetic input, not a packet
capture containing customer messages.

| Connection | Ordered reads | EOF |
| --- | --- | --- |
| a | `00`; then `02 c3`; then `a9 00 01 41` | yes |
| b | `00 01 42 00 00 00 01 43` | yes |
| c | `00 03 41 42` | yes |
| d | `00 09` | no |
| e | `00 01 ff` | no |
| f | `00 01 41`; then `00 01 42 00 01 ff` | no |

UTF-8 bytes `c3 a9` encode é; `41`, `42`, `43` encode A, B, C. Byte `ff` cannot
form a valid UTF-8 payload. The consumer's diagnostic records missing or altered
messages under v3, but there is no complete traffic inventory. Increasing the
read buffer does not impose a sender/receiver boundary guarantee in S11.
