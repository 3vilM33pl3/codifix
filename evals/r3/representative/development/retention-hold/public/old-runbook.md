# Retired archive cleanup card

The old card used nightly cleanup as the search cutoff and treated a closed
ticket as hold release. It also called a storage timeout "best-effort deleted".
P9 supersedes all three shortcuts. Preserve this card for audit context only.

Current operators must check policy at read time, consult the separate hold
registry and distinguish uncertain storage effects from confirmed deletion.
An operator display may show an expired row with a retained payload and valid
hold; those states are not contradictory.
