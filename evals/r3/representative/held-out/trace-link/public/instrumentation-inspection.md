# Instrumentation v13 inspection

The publisher stores only the current thread-local trace ID on a queue item and
drops the publishing span ID. The consumer reads its own ambient thread-local
context instead of the immutable queue carrier. A pooled consumer thread can
still contain the previous request's context. Retry logging reuses the prior
processing span ID, merging attempts in the export viewer.

A proposed patch u31 enables 100 percent sampling but leaves propagation and
retry identities unchanged. Another treats every missing span as failed work.
Neither satisfies U20. No payload logging or global production sampling change
is approved; the fixture contains synthetic identifiers only.
