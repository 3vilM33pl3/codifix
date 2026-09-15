# Lantern rollout inspection

Inspected build: `lantern-2.4.0`, 6 September 2026 at 09:25. The inspection records
running configuration, not merely the intended release manifest.

```text
multi_space_routes = true
authorize_each_request = true
answer_cache_enabled = true
answer_cache_ttl_seconds = 300
scoped_cache_v3 = false
```

The active lookup implementation hashes only the normalized question. The
response cache contains the rendered answer and citation list. It does not
partition entries by space, authorization fingerprint, corpus revision, answer
mode or model configuration. There is no cited-source eligibility recheck on a
cache hit. Route authorization does run before lookup.

With cache bypassed, the inspected retrieval path applies the requesting
project's restriction before its candidate limit. That observation does not
establish the behavior of every historical release or every other transport.

Revision 3 is approved but was not active in this process. A proposed change to
enable it has not been recorded as deployed or verified. A separate proposal
would lower the existing cache TTL from 300 seconds to 30 seconds.
