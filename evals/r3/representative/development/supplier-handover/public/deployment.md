# Post-incident configuration inspection

Read-only inspection on 2026-08-20; no change is implied by this record.

```toml
[relay]
deduplication_key = "settlement_id"
deduplication_retention_hours = 72

[supplier]
maximum_replay_age_hours = 120

[transport]
acknowledgement_timeout_seconds = 3
```

The supplier advertises support for replay up to five days old. The record above
does not include evidence of a delayed-replay acceptance test or a revised
supplier agreement. An unrelated telemetry buffer retains metrics for 24 hours;
that setting does not control settlement deduplication.
