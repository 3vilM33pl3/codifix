# N14 repair review — pending

Collect contributions keyed by stable source ID, reject duplicate/missing
expected IDs, then fold in the contract's ascending-ID order. Keep the chosen
finite-precision rule and revision explicit. Record selected-input digests and
enough execution provenance to verify the same inputs and reduction semantics.

Test permuted worker completion orders and different worker counts against the
same specified fold, including cancellation/missing results and duplicate
messages. Invalid or incomplete jobs are not comparable successes. Do not select
the best repeat or count repeats as independent new input observations.

An accuracy-focused summation method may be a separate future design, but it
must not silently replace N14 during a frozen measurement. Fixed order solves
this source of reproducibility variation; it is not proof of all-system
determinism. No production rollout or quality improvement is recorded.
