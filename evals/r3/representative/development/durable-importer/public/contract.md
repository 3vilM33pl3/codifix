# Reviewed importer contract, revision 3

Mutation, accepted identity and cursor must commit together. An injected failure
before commit changes none of them. A lost acknowledgement after commit cannot
be treated as rollback. Arithmetic overflow must reject the operation atomically.

The authority sequence, not transport arrival or lexical operation-ID order,
determines projection order. Later delivery may arrive first and wait durably
for the missing predecessor. When it arrives, apply the contiguous prefix.

An exact repeated sequence/ID/delta is a duplicate and has no extra effect.
Reusing an accepted or queued ID with another sequence/delta, or a sequence with
another ID/delta, is rejected without changing any state. Never replace the
original accepted content. A gap is not authorization to skip the missing write.

Restart retains accepted effects, identities, cursor and queued work. Volatile
status can be rebuilt, but must not reset durable ordering. The store is an
in-process durable-store test double: this exercise evaluates transaction and
replay semantics, not filesystem fsync, cryptography or real database recovery.

## Historical alternative — rejected

A previous proposal sorted records by their string keys and reset the cursor on
worker startup. Review rejected this because keys do not encode authority order,
and resetting a cursor can repeat already committed effects.
