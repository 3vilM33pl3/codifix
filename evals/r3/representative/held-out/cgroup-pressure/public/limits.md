# Memory evidence limits

The selected disjoint charges explain a limit breach without a large heap.
They do not identify an allocator leak, quantify fleet failure probability or
certify data recovery after termination. Host-free memory and group headroom
are different scopes. The fake admission check does not measure real throughput.

A leak diagnosis needs retained-allocation and time-series evidence. A capacity
recommendation needs representative peak/working-set and concurrency evidence.
An exactly-once or recovery claim needs batch/outcome records. None is supplied
here. Keep the observed group kill separate from those unresolved questions.
