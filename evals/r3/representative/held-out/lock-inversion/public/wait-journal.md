# Selected paused-thread snapshot

The debugger captured these states simultaneously in a synthetic test:

| Thread | Operation | Holds | Waiting for |
| --- | --- | --- | --- |
| a | refresh | C | I held by b |
| b | compact | I | C held by a |

Neither thread is using CPU in this snapshot. A heartbeat on an unrelated thread
still succeeds. No lock has been released and neither logical mutation has
published a completed generation. This forms a wait cycle, not a measurement of
normal lock-hold time.

In a separate callback test, thread c holds C in notify and its callback tries
to enter refresh and acquire C again. C is non-reentrant in this fixture. No
network call or database wait occurs in either test. Only these selected paths
have been inspected; there is no complete fleet deadlock inventory.
