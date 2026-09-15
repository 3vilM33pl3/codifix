# P17 repair review — pending

Implement and check each contracted persistence stage. Keep temporary files on
the selected directory's filesystem, preserve previous generation history, and
reconcile incomplete intents against actual selected generation after restart.
Distinguish pre-rename failure, post-rename uncertain durability and confirmed
durability in operator status. Repeated operation IDs must not create a second
unrelated replacement while the first is unresolved.

Test failures after each stage, including a directory flush error after a visible
rename, and verify honest status plus intact selected bytes after modeled recovery.
Do not report rollback without evidence or promote a sleep to a durability test.
No production deployment or hardware recovery certificate is recorded.
