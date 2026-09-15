# Collector inspection, rollout R12

Active collector build c31 reads numeric `temperature` but ignores `unit` and
stores that number directly in `temperature_c`. The dashboard reads this column
and displays a Celsius suffix. The alert rule compares it to85. The approved
collector change c32, which switches on unit before storage, is not deployed.

Device h1 is on firmware2.1; h2 remains on1.8. Network delivery acknowledgements
are normal for the sampled interval. No dropped-packet evidence is supplied.
The chart's time range is five minutes, not an independently calibrated physical
measurement. Changing a chart label alone would not repair stored canonical
values or alert evaluation.

Raw report retention for this synthetic packet includes the samples below. The
complete fleet's historical raw retention coverage has not yet been inventoried.
