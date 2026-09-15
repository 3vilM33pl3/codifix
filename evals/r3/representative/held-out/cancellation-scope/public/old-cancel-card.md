# Retired caller-cancellation card

The early prototype performed all rendering in the request process. Its card
treated dropped futures as stopped work and told operators to resubmit on any
timeout. That assumption does not hold for the current remote renderer.

R8 separates local waiting, explicit remote stop commands and terminal receipts.
Do not promote a successful health check, a red UI status or a stop-requested
acknowledgement into evidence that an accepted operation was undone.
