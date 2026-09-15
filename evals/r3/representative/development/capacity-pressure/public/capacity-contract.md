# Capacity contract, revision 6

Budget connections across all processes, including scheduled workers. At twelve
API replicas and a scheduler budget of 12, the API pool cap may be no more than
15 per replica if the full 192 application slots are usable. This is a connection
ceiling, not a demonstrated safe throughput target or a recommendation to use
every slot. Leave operational headroom and measure contention before increasing
admission.

Pool acquisition and SQL execution are separate phases. Increasing the statement
timeout does not lengthen the acquisition deadline. Retrying every pool timeout
immediately can amplify pressure. Admit bounded work, return explicit backpressure
and retain retry identity; do not claim a timeout means an executed job failed.

Liveness is intentionally independent of downstream readiness. On-call must
inspect pool waits, active work and lock age. A green liveness response is not an
assertion that a new job can be served promptly.
