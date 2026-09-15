# Alder worker memory budget — M13

This fictional container model charges application heap, native allocations,
resident mapped-file pages and supervisor memory to one group limit. Its
accounting categories in this case are disjoint and already reconciled; do not
add a second RSS measurement to them. All sizes are MiB, not decimal MB.

The limit is 512 MiB for the whole group. Application heap telemetry reports
only the heap category. A healthy-looking heap does not imply group headroom.
At allocation time, an additional charge beyond the group limit fails and the
fixture supervisor records a group OOM kill. Host-wide free memory does not
automatically raise this explicit group limit.

M13 requires bounded total working-set planning, bounded queued/in-flight
buffers and visible group-level pressure. Mapping a file is not an exemption
from charges when its pages are resident in this model. Do not assume every
mapped address is resident; use the supplied resident charge. The contract does
not describe every real kernel reclaim policy or every allocator.
