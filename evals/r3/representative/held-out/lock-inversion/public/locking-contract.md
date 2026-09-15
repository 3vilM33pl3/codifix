# Elm catalog locking — L15

This fictional in-memory catalog has two exclusive locks: catalog C and index I.
Operations needing both must acquire C before I, release in reverse order, and
never call an unknown callback while holding either lock. Operations must expose
neither half-updated catalog/index state nor data races. A timeout detects lack
of progress; it does not make an inconsistent partial update safe to publish.

If an operation needs external callback work, snapshot the required input under
the prescribed lock order, release locks, perform the callback and revalidate
the snapshot generation before applying results under the same lock order.
If the generation changed, reject/recompute explicitly. Do not reuse stale work
as if it reflected the new catalog. This is a synchronization contract, not a
database transaction or cross-process consensus model.
