CREATE TABLE agency_prepared_plans (
 id UUID PRIMARY KEY, mandate_id UUID NOT NULL REFERENCES agency_mandates(id),
 plan JSONB NOT NULL, changes JSONB NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
