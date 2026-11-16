CREATE TABLE agency_plan_heads (
 plan_id UUID NOT NULL, mandate_id UUID NOT NULL REFERENCES agency_mandates(id),
 revision BIGINT NOT NULL CHECK(revision>0), digest TEXT NOT NULL,
 prepared_id UUID NOT NULL, PRIMARY KEY(plan_id,mandate_id)
);
