-- Fixed codes only: never expose source content or provider response bodies.
ALTER TABLE laptop_curated_outbox ADD COLUMN blocked_reason TEXT
    CHECK(blocked_reason IS NULL OR blocked_reason IN ('invalid_curated_content','authority_denied'));
