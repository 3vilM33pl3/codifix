# Stream interpretation limits

The examples separate byte-stream transport from application framing. They do
not establish network packet loss, sender malice, actual fleet message-loss rate
or exactly-once application execution. Earlier delivered messages cannot be
recalled when a later feed fails; downstream side effects need their own contract.

S11 requires exact UTF-8 messages, so replacing invalid bytes changes meaning.
EOF and malformed input must have distinct visible outcomes from a valid empty
message. No sender retry policy, traffic denominator or performance measurement
is supplied. Correct framing alone does not establish reliable business delivery.
