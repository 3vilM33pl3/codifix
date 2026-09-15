# Juniper framed receiver

The receiver consumes arbitrary transport chunks, not one message per chunk.
Each frame is a two-byte unsigned big-endian payload length followed by that
many UTF-8 bytes. Length counts bytes, not Unicode characters. Empty payloads
are valid. Maximum payload length is eight bytes. Multiple frames may arrive in
one chunk and headers/payloads may be split across chunks, including a split
inside a UTF-8 character. Validate UTF-8 only for complete payloads.

`feed HEX` supplies up to1024 bytes; `feed -` is an empty chunk. A successful
feed prints `frames:` followed by completed payloads in lowercase hexadecimal,
separated by commas, in wire order. A single empty payload prints `frames:`;
the delivered count disambiguates it from no completed frame. Incomplete bytes
remain pending. Each complete frame increments delivered exactly once.

Reject a length above eight immediately when its complete header is present,
even if its payload has not arrived. Malformed UTF-8 in a complete payload is
also an error. A failing feed delivers none of that feed's completed frames,
clears pending bytes and poisons the connection. Frames delivered by prior
successful feeds remain counted. Further feed/end calls return `error:poisoned`.

`end` closes only a clean frame boundary; a partial header or payload returns
`error:truncated` and poisons the connection. A clean end prints `closed`; later
feed/end returns `error:closed`. `reset` starts a new logical connection, clears
pending/delivered state and prints `reset`. It is explicit operator/test input,
not automatic recovery of a corrupt stream. `status` prints exactly
`pending:N delivered:N state:open|closed|poisoned` with the actual state label.

Command syntax/hex errors print `invalid` without changing receiver state.
Odd hex lengths, nonhex characters and oversized chunks are syntax errors.
The text command shell and hex encoding are test interfaces; the framing logic
models byte streams in memory, not sockets, transport authenticity or persistence.

The archived prototype interpreted each feed as one whole message and dropped
trailing bytes. That shortcut is superseded by this contract. Preserve the
existing command protocol and public tests while repairing the receiver.
