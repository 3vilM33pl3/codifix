# Selected preflight results

All sizes are bytes. Arithmetic below records v9's unsigned 16-bit wrapping;
payload data itself is not included in this fixture.

| Bundle | Payload A | Payload B | Header | v9 returned size |
| --- | --- | --- | --- | --- |
| a | 40,000 | 30,000 | 16 | 4,480 |
| b | 32,759 | 32,760 | 16 | 65,535 |
| c | 32,760 | 32,760 | 16 | 0 |
| d | 0 | 0 | 16 | 16 |

Every payload field fits individually. a reaches the writer and fails its size
agreement check. There is no evidence that a or c produced a completed bundle.
b and d are valid-boundary fixtures in the contract tests; their acceptance does
not validate the general wrapping implementation. No population workload or
allocation exhaustion test is supplied.
