# Packer v9 inspection

Each payload length is checked individually against 65,535. The preflight then
adds header and payload lengths in a wrapping unsigned 16-bit accumulator and
checks whether that already wrapped value is at most 65,535. The writer allocates
using the returned preflight value but attempts to append every original byte.

A proposed patch o24 saturates the addition at 65,535 and accepts that result.
Another removes the 16-byte header from the size calculation. Neither makes the
returned size equal the bytes the writer intends to emit. Neither is deployed.
The observed writer reports a size mismatch and aborts; no memory corruption,
code execution or completed malformed output has been demonstrated.
