# Parser v3 inspection

v3 processes each transport read independently. It interprets the first two
bytes as a length, passes the remainder of that read to UTF-8 conversion and
emits one message. It neither retains an incomplete suffix nor loops over
multiple frames. Reads shorter than two bytes are silently ignored. The parser
assumes an editor's ASCII examples represent all valid inputs.

A proposed patch f14 increases the socket read buffer. It does not add decoder
state, loop over complete frames or enforce the eight-byte declared length.
Another patch replaces invalid UTF-8 bytes with a replacement character. Neither
patch is approved. The application consumer requires exact payload text, not a
best-effort repaired message. No network reordering or packet-loss evidence is
attached; the journal lists bytes in the order supplied to the decoder.
