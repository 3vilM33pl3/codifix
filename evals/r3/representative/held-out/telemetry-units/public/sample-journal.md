# R12 raw and canonical samples

At10:00 h1 reports value68, unitF, firmware2.1; c31 stores temperature_c=68.
At10:01 h1 reports value194, unitF, firmware2.1; c31 stores temperature_c=194.
At10:02 h2 reports value86, unitC, firmware1.8; c31 stores temperature_c=86.
At10:03 test device hx reports value50, unitX; c31 stores temperature_c=50.

The alert engine emitted warnings for h1 at10:01 and h2 at10:02. No warning is
recorded for h1 at10:00. Under T6,68F is20C and194F is90C; the latter still
exceeds the85C warning threshold. h2's86C also remains a warning. hx's unit is
undefined in the contract and has no justified canonical Celsius value.

There is no independent sensor calibration, physical damage inspection or
historical alert census in this packet. The samples establish representation
errors, not proof of damage or harmlessness throughout the fleet.
