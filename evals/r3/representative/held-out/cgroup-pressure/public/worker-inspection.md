# Worker v6 inspection

The worker sets a 128-MiB heap target and logs heap usage as its `memory total`.
It opens a mapped data file, maintains native decompression buffers and starts
one buffer per admitted batch without a group-wide admission budget. Its queue
is bounded by batch count, not the resident bytes of each batch.

A proposed patch m19 lowers only the heap target. Another calls garbage
collection when host-free memory drops below a threshold. Neither observes
native/mapped/supervisor charges or changes batch admission. No retained
heap-object-growth profile is provided. A low heap measurement is not evidence
that native or mapped memory is uncharged, nor proof of a heap leak.
