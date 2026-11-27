-- Additive, opt-in agency. Existing memory workers exclude specialist jobs.
ALTER TABLE background_jobs DROP CONSTRAINT background_jobs_kind_check;
ALTER TABLE background_jobs ADD CONSTRAINT background_jobs_kind_check
 CHECK (kind IN ('embedding','semantic_dedup','validation_due','consolidation','consolidation_synthesis','specialist_task'));

CREATE TABLE agency_mandates (
 id UUID PRIMARY KEY, project_id UUID NOT NULL REFERENCES projects(id),
 subject_id TEXT NOT NULL, parent_id UUID REFERENCES agency_mandates(id),
 document JSONB NOT NULL, revision BIGINT NOT NULL DEFAULT 1,
 paused BOOLEAN NOT NULL DEFAULT false, revoked BOOLEAN NOT NULL DEFAULT false,
 used_microusd BIGINT NOT NULL DEFAULT 0 CHECK(used_microusd >= 0),
 used_calls BIGINT NOT NULL DEFAULT 0 CHECK(used_calls >= 0),
 reserved_microusd BIGINT NOT NULL DEFAULT 0 CHECK(reserved_microusd >= 0),
 reserved_calls BIGINT NOT NULL DEFAULT 0 CHECK(reserved_calls >= 0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE agency_tasks (
 id UUID PRIMARY KEY, project_id UUID NOT NULL REFERENCES projects(id),
 mandate_id UUID NOT NULL REFERENCES agency_mandates(id), requester TEXT NOT NULL,
 request_id TEXT NOT NULL, parent_id UUID REFERENCES agency_tasks(id),
 contract JSONB NOT NULL, payload JSONB NOT NULL,
 revision BIGINT NOT NULL DEFAULT 1, job_id UUID REFERENCES background_jobs(id),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(requester,request_id), CHECK(pg_column_size(payload) <= 1048576)
);
CREATE TABLE agency_messages (
 task_id UUID NOT NULL REFERENCES agency_tasks(id), message_id TEXT NOT NULL,
 sender TEXT NOT NULL, body JSONB NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(task_id,message_id), CHECK(pg_column_size(body)<=262144)
);
CREATE TABLE agency_steps (
 id UUID PRIMARY KEY, task_id UUID NOT NULL REFERENCES agency_tasks(id),
 receipt JSONB NOT NULL, updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE agency_schedules (
 id UUID PRIMARY KEY, mandate_id UUID NOT NULL REFERENCES agency_mandates(id),
 requester TEXT NOT NULL, interval_seconds INTEGER NOT NULL CHECK(interval_seconds BETWEEN 30 AND 604800),
 next_at TIMESTAMPTZ NOT NULL, template JSONB NOT NULL, enabled BOOLEAN NOT NULL DEFAULT false
);
CREATE TABLE agency_outcomes (
 task_id UUID PRIMARY KEY REFERENCES agency_tasks(id), evidence JSONB NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE agency_improvements (
 id UUID PRIMARY KEY, specialist_id TEXT NOT NULL, document JSONB NOT NULL,
 revision BIGINT NOT NULL DEFAULT 1, updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
