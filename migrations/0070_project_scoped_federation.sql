-- Project scopes do not change schema-v2/v3 publication bytes. Only the
-- authenticated origin may bind its streams; the gateway receipt attests this
-- explicit project policy, not an independently verifiable complete repo.
CREATE TABLE federation_project_feeds (
    space UUID NOT NULL,
    owner UUID NOT NULL,
    project UUID NOT NULL,
    company UUID NOT NULL,
    area TEXT NOT NULL CHECK (area IN ('workshop','homelab','general')),
    revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
    catalog_revision BIGINT NOT NULL CHECK (catalog_revision > 0),
    policy_revision BIGINT NOT NULL CHECK (policy_revision > 0),
    head BIGINT NOT NULL DEFAULT 0 CHECK (head >= 0),
    retained_after BIGINT NOT NULL DEFAULT 0 CHECK (retained_after >= 0),
    PRIMARY KEY (space,owner,project)
);
CREATE TABLE federation_project_streams (
    space UUID NOT NULL,
    owner UUID NOT NULL,
    canonical UUID NOT NULL,
    project UUID NOT NULL,
    review JSONB NOT NULL,
    PRIMARY KEY (space,owner,canonical),
    FOREIGN KEY (space,owner,project) REFERENCES federation_project_feeds(space,owner,project)
);
CREATE TABLE federation_project_events (
    space UUID NOT NULL,
    owner UUID NOT NULL,
    project UUID NOT NULL,
    sequence BIGINT NOT NULL CHECK (sequence > 0),
    event JSONB NOT NULL,
    PRIMARY KEY (space,owner,project,sequence),
    FOREIGN KEY (space,owner,project) REFERENCES federation_project_feeds(space,owner,project)
);
CREATE TRIGGER federation_project_events_immutable BEFORE UPDATE OR DELETE ON federation_project_events
FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();
CREATE TABLE federation_project_operations (
    space UUID NOT NULL,
    owner UUID NOT NULL,
    operation_id UUID NOT NULL,
    request_digest JSONB NOT NULL,
    result JSONB NOT NULL,
    PRIMARY KEY (space,owner,operation_id)
);
CREATE TRIGGER federation_project_operations_immutable BEFORE UPDATE OR DELETE ON federation_project_operations
FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();
-- Separate laptop read-only projection and atomic per-subscription cursor.
-- No row is written into memory_entries; local write routes cannot edit these.
CREATE TABLE onboarding_project_projections (
    subscription UUID PRIMARY KEY,
    device UUID NOT NULL,
    scope JSONB NOT NULL,
    cursor BIGINT NOT NULL DEFAULT 0 CHECK (cursor >= 0),
    projection JSONB NOT NULL,
    observed_at TIMESTAMPTZ
);
CREATE TABLE onboarding_project_pages (
    subscription UUID NOT NULL REFERENCES onboarding_project_projections(subscription),
    nonce UUID NOT NULL,
    page_digest JSONB NOT NULL,
    through_cursor BIGINT NOT NULL,
    PRIMARY KEY (subscription,nonce)
);
CREATE TRIGGER onboarding_project_pages_immutable BEFORE UPDATE OR DELETE ON onboarding_project_pages
FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();
