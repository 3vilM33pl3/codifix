# Iris message stream — S11

This fictional byte protocol encodes each message as a two-byte unsigned
big-endian payload length followed by exactly that many UTF-8 payload bytes.
The maximum payload is eight bytes. A zero-length payload is a valid empty
message. Length counts bytes, not Unicode characters. Header and payload bytes
may arrive across any number of transport reads; one read may contain several
complete messages. Transport-read boundaries have no message meaning.

Maintain per-connection decoder state. Buffer incomplete headers and payloads,
emit complete messages in order and retain only the incomplete suffix. Decode
UTF-8 only after the complete payload arrives. A length above eight or an invalid
complete UTF-8 payload fails that connection explicitly; do not truncate, scan
for a guessed next header or continue after failure. Discard a failing feed's
new messages as a unit; messages delivered by earlier successful feeds remain.

EOF with no pending bytes is clean. EOF with a partial header or payload is
truncated-stream failure, not an empty message. A new connection starts fresh
state; bytes from one connection must not complete another connection's frame.
