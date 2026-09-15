# Violet catalog sample

Account11: label `Café` with final U+00E9, current key `café`, grants read-project-a.
Account12: label `Café` with final U+0065 U+0301, current key `café`, grants
manage-project-b. Account13: label `Cafe`, current key `cafe`, grants read-project-c.
These are three stable IDs with different grants and audit histories.

The first two keys were generated under mixed historical writer behavior, not a
single U4-compatible index. U4 maps both acute-e spellings to `café`; plain `Cafe`
still maps to `cafe`. No account merge or grant transfer was authorized.

A proposed fourth label `Сafe` starts with Cyrillic U+0421, not Latin C. It is
outside U4's allowed alphabet. A similar font appearance does not change its
code point or authorize a different account identity.
