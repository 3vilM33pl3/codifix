# Thermal rollout repair review

Deploy the reviewed unit-aware collector after tests with C, F and unknown units.
Verify ordinary and boundary temperatures across mixed firmware; preserve valid
warnings and quarantine unknown units. Store original report and canonical value
with the conversion revision so repeated processing does not convert twice.

For history, use each raw value/unit and provenance to identify affected rows.
Recompute only rows whose conversion status is known; quarantine ambiguous rows
instead of guessing from the dashboard suffix or magnitude. Preserve the source
report and audit the repair. Do not claim all history is fixed before raw-coverage
inventory and reconciliation are complete.

Check physical safety using the appropriate operational process independently
of chart repair. The evidence does not determine a fleet-wide damage count,
historical false-positive rate or sensor calibration accuracy.
