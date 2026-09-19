// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import { summarizeLineage, annotateMixedSources, redact, consumeReview, REVIEW_SQL, LIMITS, parseArgs } from './classify.mjs';

const cid = '00000000-0000-0000-0000-000000000001';
const other = '00000000-0000-0000-0000-000000000002';
const project = (slug = 'ca-2') => ({ id: cid, slug, name: slug, root_path: `/synthetic/${slug}`, created_at: '2026-09-17T00:00:00Z' });
const fixture = (text, slug = 'ca-2', canonical = cid) => ({ canonical_id: canonical, state: { status: 'active' }, projects: [project(slug)],
  versions: [{ id: canonical, project_id: cid, version_no: 1, is_tombstone: false, summary: 'Synthetic review title', canonical_text: text,
    row_sha256: 'a'.repeat(64), memory_type: 'implementation', scope: 'project', created_at: '2026-09-17T00:00:00Z', updated_at: '2026-09-17T00:00:00Z' }], sources: [] });

test('ca-2 does not become Trust from its name; explicit CA content can move from Homelab', () => {
  const career = summarizeLineage(fixture('Recruiter interview preparation for a technology leadership role.'));
  assert.equal(career.proposal.candidate_area, 'general');
  assert.ok(career.proposal.reasons.some(reason => reason.includes('not certificate-authority evidence')));
  const trust = summarizeLineage(fixture('Configure step-ca intermediate CA certificate issuance.', 'homelab'));
  assert.equal(trust.proposal.candidate_area, 'trust');
  assert.equal(trust.proposal.ownership_approved, false);
});

test('generic memory, certificate, Docker or tests do not independently create specialized ownership', () => {
  const entry = summarizeLineage(fixture('Test the Docker certificate renewal and use memory efficiently.', 'misc'));
  assert.equal(entry.proposal.candidate_area, 'general');
  assert.equal(entry.proposal.source_classification, 'knowledge');
});

test('specific Workshop/Homelab markers and original project boundaries are retained', () => {
  const workshop = summarizeLineage(fixture('Change mem-service endpoints and Memory Layer schema checks.', 'memory'));
  assert.equal(workshop.proposal.candidate_area, 'workshop');
  assert.deepEqual(workshop.original_projects, [project('memory')]);
  assert.equal(summarizeLineage(fixture('PipeWire audio recovery on Monolith.', 'homelab')).proposal.candidate_area, 'homelab');
});

test('synthetic/evaluation project contamination is a protected unapproved proposal', () => {
  const evaluation = summarizeLineage(fixture('Memory Layer result', 'eval-r3-case-42'));
  assert.equal(evaluation.proposal.suggested_disposition, 'protected_archive_candidate');
  assert.equal(evaluation.proposal.source_classification, 'evaluation');
  const text = summarizeLineage(fixture('This is a synthetic fixture record.', 'memory'));
  assert.equal(text.proposal.suggested_disposition, 'protected_archive_candidate');
  assert.equal(text.proposal.ownership_approved, false);
});

test('real delimited test/eval/smoke project names create explicit false-positive review holds', () => {
  for (const slug of ['memory-bundle-smoke', 'memory-quality-eval', 'keyless-gate-test', 'ingest-test', 'skyrim-test',
    'memory_test_case', 'memory.fixture.case', 'memory:benchmark:case', 'eval/r3', 'tests']) {
    const entry = summarizeLineage(fixture('Ordinary retained project notes.', slug));
    assert.equal(entry.proposal.suggested_disposition, 'protected_archive_candidate', slug);
    assert.ok(entry.proposal.risks.includes('project_marker_requires_false_positive_review'), slug);
    assert.equal(entry.proposal.ownership_approved, false);
    assert.deepEqual(entry.original_projects, [project(slug)]);
  }
  for (const slug of ['latest', 'contest', 'testimony', 'evaluationism']) {
    assert.equal(summarizeLineage(fixture('Ordinary project notes.', slug)).proposal.suggested_disposition, 'area_candidate', slug);
  }
});

test('Memory Layer initialization in SCTP, Rillforge and CanalGame does not relocate project ownership', () => {
  for (const slug of ['SCTP', 'Rillforge', 'CanalGame']) {
    const entry = summarizeLineage(fixture('Initialize Memory Layer in this project with memory-layer and mem-cli.', slug));
    assert.equal(entry.proposal.candidate_area, 'general', slug);
    assert.equal(entry.proposal.confidence, 'original_project_preserved');
    assert.ok(entry.proposal.risks.includes('memory_layer_use_does_not_establish_workshop_ownership'));
    assert.ok(entry.proposal.risks.includes('content_topic_differs_from_original_project_area'));
    assert.deepEqual(entry.original_projects, [project(slug)]);
  }
});

test('Turris admin certificate consumers and bare CA or PKI topics cannot become Trust', () => {
  for (const text of ['Turris admin UI HTTPS certificate issued by the certificate authority; PKI trust chain.',
    'Configure step-ca for a Turris admin interface certificate consumer.',
    'The certificate authority is part of our PKI.', 'Document step-ca certificate authority concepts.']) {
    const entry = summarizeLineage(fixture(text, 'homelab'));
    assert.equal(entry.proposal.candidate_area, 'homelab', text);
    assert.ok(entry.proposal.risks.includes('possible_certificate_authority_topic'));
  }
  const consumer = summarizeLineage(fixture('Configure step-ca for the application dashboard certificate.', 'homelab'));
  assert.ok(consumer.proposal.risks.includes('certificate_consumer_not_authority_ownership'));
});

test('negated CA operator guidance and CA mentions in other projects preserve their original area', () => {
  for (const slug of ['homelab', 'memory', 'ca-2']) {
    const entry = summarizeLineage(fixture('Never configure step-ca or rotate the intermediate CA.', slug));
    assert.equal(entry.proposal.candidate_area, { homelab: 'homelab', memory: 'workshop', 'ca-2': 'general' }[slug]);
    assert.ok(entry.proposal.risks.includes('negated_authority_operation_requires_review'));
  }
  assert.equal(summarizeLineage(fixture('Configure step-ca intermediate CA certificate issuance.', 'memory')).proposal.candidate_area, 'workshop');
});

test('all historical versions participate; tombstones and cross-area histories cannot disappear', () => {
  const input = fixture('Maintain the certificate authority.', 'homelab');
  input.versions.push({ ...input.versions[0], id: other, version_no: 2, canonical_text: 'Change mem-service and Memory Layer.', summary: 'New topic' });
  const mixed = summarizeLineage(input);
  assert.equal(mixed.history.version_count, 2);
  assert.equal(mixed.proposal.source_classification, 'mixed');
  assert.equal(mixed.proposal.suggested_disposition, 'protected_archive_candidate');
  assert.equal(mixed.proposal.candidate_area, 'homelab');
  input.versions.push({ ...input.versions[0], id: '00000000-0000-0000-0000-000000000003', version_no: 3, canonical_text: '', summary: '', is_tombstone: true });
  const tombstone = summarizeLineage(input);
  assert.equal(tombstone.history.tombstone_count, 1);
  assert.equal(tombstone.latest_readable_version_id, other);
});

test('possible private keys and credentials are redacted and kept in protected review', () => {
  const secret = '-----BEGIN PRIVATE KEY-----\nvery-secret-fixture\n-----END PRIVATE KEY-----';
  const entry = summarizeLineage(fixture(`step-ca ${secret}`, 'homelab'));
  assert.equal(entry.proposal.suggested_disposition, 'protected_archive_candidate');
  assert.ok(!JSON.stringify(entry).includes('very-secret-fixture'));
  assert.ok(JSON.stringify(entry).includes('REDACTED'));
  assert.ok(redact('sk-proj-' + 'a'.repeat(32)).redacted);
});

test('source previews and text are bounded with explicit omissions; reference IDs survive', () => {
  const input = fixture('A'.repeat(LIMITS.entry_excerpt_chars + 100));
  input.sources = Array.from({ length: 9 }, (_, index) => ({ id: `source-${index}`, memory_entry_id: cid, task_id: null, session_id: null,
    file_path: '/source', source_kind: 'file', excerpt: 'B'.repeat(600), capture_ids: [], row_sha256: 'b'.repeat(64) }));
  const entry = summarizeLineage(input);
  assert.equal(entry.content_excerpt.text.length, LIMITS.entry_excerpt_chars);
  assert.equal(entry.content_excerpt.abbreviated, true);
  assert.equal(entry.source_references.length, 9);
  assert.equal(entry.source_previews.length, LIMITS.source_previews);
  assert.equal(entry.source_previews_omitted, 3);
});

test('shared sessions/tasks spanning candidates propose archive without silently approving source copies', () => {
  const first = fixture('step-ca certificate authority procedure.', 'homelab');
  const second = fixture('Recruiter interview preparation.', 'ca-2', other);
  for (const input of [first, second]) input.sources.push({ id: `source-${input.canonical_id}`, memory_entry_id: input.canonical_id,
    task_id: 'shared-task', session_id: 'shared-session', excerpt: '', source_kind: 'task', capture_ids: ['capture-1'], row_sha256: 'a'.repeat(64) });
  const entries = [summarizeLineage(first), summarizeLineage(second)];
  const archive = annotateMixedSources(entries);
  assert.equal(archive.length, 2);
  assert.ok(archive.every(item => item.approved === false));
  assert.ok(entries.every(entry => entry.proposal.risks.includes('shared_source_may_mix_areas')));
});

test('stream verifies exact source counts and manifest lineage coverage before publishing', async () => {
  const bindings = { canonical_ids: new Set([cid]), expected_versions: 1, expected_projects: 1, container_id: 'container', evidence: {} };
  const events = [{ event: 'header', read_only: 'on', database: 'memory_restore', canonical_count: 1, version_count: 1, project_count: 1 },
    { event: 'lineage', ...fixture('Career knowledge.') }, { event: 'complete' }];
  const lines = values => (async function* () { for (const event of values) yield JSON.stringify(event); })();
  const pack = await consumeReview(lines(events), bindings);
  assert.equal(pack.coverage.canonical_histories, 1);
  assert.equal(pack.rules_version, 2);
  assert.equal(pack.ownership_approved, false);
  assert.equal(pack.import_supported, false);
  await assert.rejects(consumeReview(lines(events.slice(0, -1)), bindings), /incomplete/);
  await assert.rejects(consumeReview(lines([events[0], events[1], events[1], events[2]]), bindings), /duplicate/);
  events[0].read_only = 'off';
  await assert.rejects(consumeReview(lines(events), bindings), /safety/);
});

test('SQL never fetches raw capture payloads or updates source state; options cannot select a live DB', () => {
  assert.match(REVIEW_SQL, /REPEATABLE READ READ ONLY/);
  assert.ok(!/payload_json|INSERT INTO|UPDATE public|DELETE FROM/.test(REVIEW_SQL));
  assert.throws(() => parseArgs(['--database', 'live']), /explicit|Explicit/);
  assert.throws(() => parseArgs([]), /six/);
});
