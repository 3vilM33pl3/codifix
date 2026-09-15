# Hazel thermal telemetry contract T6

Devices publish a numeric temperature and an explicit unit field. Firmware1.8
emits Celsius (`C`); firmware2.1 emits Fahrenheit (`F`). Both firmware versions
are allowed during this rollout. Identity includes the device, firmware version
and sample timestamp; a dashboard label is not a source unit declaration.

The collector stores canonical Celsius and retains the reported value/unit as
provenance. Convert F to C using (F-32)*5/9 exactly once. Values already reported
as C pass through unchanged. Unknown units are quarantined with an explicit
reason; they must not be guessed from the numeric magnitude.

The operational warning threshold is85 Celsius, inclusive. A conversion repair
must not disable real warnings or apply Fahrenheit conversion to all devices.
Historical canonical rows require provenance-aware repair, not a blind conversion
of every stored temperature. Original reports remain preserved.
