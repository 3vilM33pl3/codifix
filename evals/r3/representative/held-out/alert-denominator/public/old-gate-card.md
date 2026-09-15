# Retired trace-based release card

The prototype used uniformly sampled traces and compared the displayed error
percentage with the release threshold. After failure-biased sampling was added,
that shortcut became invalid. M6 requires unsampled counters or explicit valid
weighting, with the same interval and population definition.

The old card also averaged consecutive window percentages without considering
their request volumes. Retain it for historical interpretation only. A confident
chart label does not change a sample's selection mechanism.
