# Linden maintenance occupancy

The calendar stores the union of occupied half-open integer intervals [start,end).
Ticks are abstract ordered positions, not civil time, timezone or wall-clock
instants. Endpoints are signed64-bit integers and start must be strictly less
than end. Do not iterate over every tick; endpoints can be far apart.

`add START END` adds occupied positions and prints `ok`. Overlapping and adjacent
intervals coalesce; repeated adds do not duplicate occupancy. Stored intervals
are sorted by start, nonempty, disjoint and nonadjacent. A containing interval
can absorb several previous spans. `show` prints `busy:` followed by `start:end`
spans joined by commas. Empty state prints `busy:`.

`remove START END` removes occupancy only inside the given half-open interval,
prints `ok`, and may trim, split or delete existing spans. Touching at an endpoint
alone removes nothing. Removing absent occupancy is successful and unchanged.

`free START END` prints `free:` followed by the canonical uncovered spans INSIDE
the requested half-open window. Busy intervals outside the window do not affect
it. Clip spanning intervals to the window. This read must not mutate the calendar
or leak portions outside the requested range.

Syntax, integer overflow while parsing, empty/reversed intervals and extra tokens
print `invalid` with no state change. Do not use endpoint +/-1 arithmetic: the
half-open model needs no such adjustment and must support i64 extremes. `clear`
empties state and prints `cleared`. Commands are a deterministic in-memory test
interface, not persistence or a production appointment system.

The archived prototype appended overlapping bookings separately, removed every
span which touched a removal, and returned the entire query window as free if
no exact booking matched. That prototype is superseded by this set contract.
