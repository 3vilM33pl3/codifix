# Cedar dispatch windows, revision 5

The fictional dispatch service accepts scheduled collections at local civil time
01:30 in the customer's named zone. The zone is stored with each schedule; it is
not the service host's zone. The scheduler resolves each date using its pinned
zone-rule table, not a permanently stored UTC offset.

If 01:30 occurs twice, the contract selects the first occurrence (earlier instant)
and executes that schedule/date once. If 01:30 does not exist, skip that day's
occurrence with a visible reason; do not shift to 02:30 silently. A skipped day
does not cancel the next day's occurrence. These are product choices, not general
rules for every scheduling system.

Execution identity is schedule ID plus intended local date. UTC time is a
resolved execution instant, not the stable identity. Completed occurrences are
not executed again when the rule table or process changes.
