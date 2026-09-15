# R12 raw and canonical samples

At10:00 h1 reports value68, unitF, firmware2.1; c31 stores temperature_c=68.
At10:01 h1 reports value194, unitF, firmware2.1; c31 stores temperature_c=194.
At10:02 h2 reports value86, unitC, firmware1.8; c31 stores temperature_c=86.
At10:03 test device hx reports value50, unitX; c31 stores temperature_c=50.

The alert engine emitted warnings for h1 at10:01 and h2 at10:02. No warning is
recorded for h1 at10:00. These are the original c31 decisions, before any
provenance-aware recomputation. Source values and unit fields above remain the
inputs for checking those decisions against T6.

There is no independent sensor calibration, physical damage inspection or
historical alert census in this packet. The samples establish representation
errors, not proof of damage or harmlessness throughout the fleet.
