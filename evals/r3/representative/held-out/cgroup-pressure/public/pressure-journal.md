# Selected group accounting sample

Immediately before the failing batch, the reconciled charges are:

| Category | MiB |
| --- | --- |
| Application heap | 96 |
| Native buffers | 140 |
| Resident mapped pages | 220 |
| Supervisor | 24 |

The next batch requests an additional 48 MiB of native buffer. The group limit
is 512 MiB; host-free memory is reported as 8 GiB. The supervisor then records
`group_oom_kill`, and the worker process exits. No before/after heap growth series,
completed batch receipt or data-integrity check is attached.

In a separate fake admission test, the 48-MiB batch is held pending until enough
charged bytes are released. Its outcome tests admission accounting only, not
actual page reclamation or production throughput. The original workload's
partial external effects are not known from a killed process alone.
