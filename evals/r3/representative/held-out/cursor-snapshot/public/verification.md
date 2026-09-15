# L4 repair verification

Check tied primary sort values, the direction of each sort component and strict
continuation. For S20 the severity decrease OR same-severity larger-ID relation
selects the next rows. Do not use one greater-than comparison for both columns
when their sort directions differ. Bind snapshot and project before execution.

Test concurrent inserts and severity edits without changing existing snapshot
pages, then start a new snapshot to expose them. Verify expiry and mismatched
project fail visibly instead of returning a plausible page from a different
listing. Assert row identities and boundaries, not just count or HTTP status.

The packet proposes this repair but records no deployed r19 or completed customer
export. Five is the Iris S20 count, not the current live count or a global total.
