# r-17 recovery review

Show the caller timeout separately from renderer state. Retain the original
operation and stop-requested receipt; label the terminal outcome unresolved.
Continue bounded reconciliation through the dedicated worker. Repeated status
reads do not authorize fresh renders or fabricate a final receipt.

Test three distinct cases: cancel before submission; cancel after running begins;
completion racing with cancellation. Assert both local response and remote outcome
without assuming their order. An already-completed response preserves completed
work; any desired deletion/compensation is a separate authorized operation.

The packet lacks terminal status and measured renderer usage for r-17, so it
does not establish a refund, final charge or cancellation latency. No general
maximum stop time is specified by the renderer contract in this packet.
