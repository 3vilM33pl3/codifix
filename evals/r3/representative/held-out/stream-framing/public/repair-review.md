# S11 implementation review — pending

Introduce a bounded stateful decoder per connection, validate declared lengths
before buffering payload, and process every complete frame in order. Keep UTF-8
validation at complete-payload boundaries. Enforce explicit poison/failure state
until connection reset; fail the entire current feed without retracting earlier
delivered messages. Do not retry malformed bytes as another protocol version.

Test all byte-boundary splits of a valid stream and multiple frames in one read.
Include empty messages, multibyte text, incomplete EOF, oversize lengths, invalid
UTF-8 and failure after previously successful delivery. Confirm reset cannot
carry bytes into a new connection. A larger receive buffer can be an independent
performance choice; it is not the correctness repair.

No release or measured throughput result is recorded. Protocol failure is not
permission to silently drop selected bytes and claim delivery success. This
bounded decoder is not a delegated-work or retry/idempotency protocol.
