# Lumen measurement export — contract Q6

This is a fictional sensor reporting service, not a financial ledger or a safety
controller. Original readings are signed integers in micro-units. They must stay
unchanged in storage. A report includes every accepted reading for one named
batch; a negative reading is a legitimate signed correction, not missing data.

Q6 accumulates the original micro-unit integers in a checked wide accumulator,
then converts the batch total to milli-units once. There are 1,000 micro-units in
one milli-unit. At the final conversion only, round to the nearest milli-unit;
an exact half goes away from zero. Thus a total of -1,500 becomes -2 milli-units.
The displayed number is not a replacement for the raw total. Store the batch ID,
raw count, raw total, unit and conversion revision with the export.

Reject overflow explicitly. Do not drop readings to make the accumulator fit or
wrap its value. Q6 does not specify a maximum population or prove a sensor's
physical accuracy. Historical reports without raw readings cannot be inverted
uniquely from a rounded display.
