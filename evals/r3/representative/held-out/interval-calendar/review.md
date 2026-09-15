# Held-out coding family: interval calendar — DRAFT

Five Rust modules and two ordinary public tests describe in-memory occupancy
over abstract signed integer ticks. This is set algebra, not the development
civil-time family: there are no zones, scheduling clocks or external dispatches.

1. Three probes cover unsorted adjacent spans, containing/duplicate additions and
   disjoint negative positions. Occupancy must have one canonical representation.
2. Three probes cover splitting, endpoint contact, trimming and removals crossing
   several spans. Ignoring removal and deleting all overlaps both fail.
3. Four probes cover clipped free windows, fully occupied windows, i64 extremes
   and non-mutating invalid queries. All earlier checks remain cumulative.

Private reference implementation and all ten probes stay outside the agent
workspace. Any implementation obeying the public contract is acceptable. No-op
and starter controls must fail each stage, and the reference must pass all.
Keep this repository held-out and do not use results to tune development prompts.
Labels remain draft and require later approval; no paid execution is implied.
