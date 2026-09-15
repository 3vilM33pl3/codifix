# Fir campaign K9 ledger at interruption

Approved total campaign cap: 1,000 microcredits. Preparation settled: 200.
Execution request A reserved 120 and settled 80; B reserved 150 and settled 110.
Request C reserved 180 and remains uncertain after a lost provider response.
There are no other reservations or settled calls in the ledger.

Thus settled preparation plus execution is 390. C's possible charge is not yet
known; its full 180 remains committed to budget safety. A proposed next request
D has a worst-case allowance of 450. Its actual future usage is not known.

The dashboard's green "execution actual=190" is A+B only, not preparation or
uncertain allowance. A label "actual" does not justify omitting those other
obligations when deciding whether to start work.
