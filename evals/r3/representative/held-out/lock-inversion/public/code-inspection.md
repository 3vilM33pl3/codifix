# Catalog v8 path inspection

`refresh` acquires C, computes a new entry, then requests I before publishing
both projections. `compact` acquires I, scans its index entries, then requests C
before removing obsolete catalog rows. Both keep their first lock while waiting
for the second. Neither path has a timeout or shared total lock-order helper.

A separate `notify` path invokes a configurable callback while holding C. The
callback may re-enter `refresh`. Its observed test callback does so. A proposed
patch l23 adds a five-second wait then writes only the projection whose lock it
holds. Another proposal removes I locking. Neither preserves L15 atomicity or
race freedom; neither has been approved or deployed.
