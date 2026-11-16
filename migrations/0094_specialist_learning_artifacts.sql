-- Immutable deployment payloads and task-bound measured rollout evidence.
-- Existing lifecycle documents remain in agency_improvements.
CREATE TABLE agency_learning_artifacts (
 candidate_id UUID PRIMARY KEY REFERENCES agency_improvements(id),
 workload TEXT NOT NULL,
 artifact JSONB NOT NULL,
 artifact_digest TEXT NOT NULL CHECK(length(artifact_digest)=64),
 config_digest TEXT NOT NULL CHECK(length(config_digest)=64),
 baseline_digest TEXT NOT NULL CHECK(length(baseline_digest)=64),
 baseline_candidate_id UUID REFERENCES agency_improvements(id),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE agency_learning_heads (
 specialist_id TEXT NOT NULL,
 workload TEXT NOT NULL,
 baseline_candidate_id UUID REFERENCES agency_improvements(id),
 canary_candidate_id UUID REFERENCES agency_improvements(id),
 revision BIGINT NOT NULL DEFAULT 1,
 PRIMARY KEY(specialist_id,workload)
);
CREATE TABLE agency_learning_evaluations (
 evidence_id TEXT PRIMARY KEY,
 verifier_id TEXT NOT NULL,
 candidate_id UUID NOT NULL UNIQUE REFERENCES agency_improvements(id),
 artifact_digest TEXT NOT NULL CHECK(length(artifact_digest)=64),
 baseline_digest TEXT NOT NULL CHECK(length(baseline_digest)=64),
 document JSONB NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE agency_learning_assignments (
 task_id UUID NOT NULL REFERENCES agency_tasks(id),
 task_revision BIGINT NOT NULL,
 binding JSONB NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(task_id,task_revision)
);
CREATE TABLE agency_learning_observations (
 task_id UUID PRIMARY KEY REFERENCES agency_tasks(id),
 task_revision BIGINT NOT NULL,
 candidate_id UUID NOT NULL REFERENCES agency_improvements(id),
 arm TEXT NOT NULL CHECK(arm IN ('candidate','baseline')),
 operational_passed BOOLEAN NOT NULL,
 evidence_digest TEXT NOT NULL CHECK(length(evidence_digest)=64),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE agency_learning_assessments (
 task_id UUID PRIMARY KEY REFERENCES agency_learning_observations(task_id),
 candidate_id UUID NOT NULL REFERENCES agency_improvements(id),
 evidence_id TEXT NOT NULL UNIQUE,
 dataset_id TEXT NOT NULL,
 case_id TEXT NOT NULL,
 document JSONB NOT NULL,
 quality_passed BOOLEAN NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(candidate_id,dataset_id,case_id)
);
