# Runner v7 inspection

The runner sorts selected source IDs before launching work, then accumulates
results as workers finish. It records the selection seed but neither completion
order nor reduction revision. A faster worker can therefore change addition
order despite identical selected inputs. The current loop has no duplicate-ID
check and accepts a repeated successful worker message as another contribution.

A proposed patch n21 fixes worker count at three but retains completion-order
accumulation. Another picks the largest score across repeated runs and labels
it `stable`. Neither implements N14. No rule authorizes dropping a negative
contribution or changing the benchmark to make observed scores agree.
