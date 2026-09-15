# Q18 repair review — pending

Correlate completion project and item ID to the authorized outer item using
`NOT EXISTS`, keeping active/project eligibility before limits. A null completion
does not equal an item ID and therefore cannot satisfy that matching predicate.
Keep unresolved imports intact and visible to their authorized review workflow.

Test null-containing, empty and duplicate completion sets, cross-project identical
IDs, inactive items and pagination over the eligible pending set. Verify lab
listing never returns game items and that a game completion cannot suppress a
lab item. Do not label a query error/unknown state as proven all-complete.
No production migration, deletion or approved rollout is recorded.
