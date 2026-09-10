#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Deterministic draft materialization. Never changes labels to reviewed.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const destination = process.argv[2];
if (!destination) throw Error('usage: node build-review-pack.mjs NEW_OUTPUT_DIRECTORY');
const root = path.resolve(destination);
if (fs.existsSync(root)) throw Error('output exists; never overwrite reviewed material');
fs.mkdirSync(root, { recursive: true });
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const assets = [];
function write(relative, content, kind) {
  const bytes = typeof content === 'string' ? content : JSON.stringify(content, null, 2) + '\n';
  const target = path.join(root, relative);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, bytes, { flag: 'wx' });
  if (kind) assets.push({ path: relative, sha256: hash(bytes), kind });
}
const knowledge = JSON.parse(fs.readFileSync(path.join(here, 'knowledge-catalog.json'), 'utf8'));
const coding = JSON.parse(fs.readFileSync(path.join(here, 'coding-catalog.json'), 'utf8'));
const cases = [];
const labels = [];
const seeds = [];
const semanticTime = '2026-09-10T00:00:00Z';
function suite(name, items) {
  write(`private/suites/${name}/suite.toml`, `name = "${name}"\nsuite_version = "r3-draft-1"\nlabel_status = "draft"\nproject = "r3-${name}"\ndefault_profile = "offline"\ndefault_repeats = 3\nitems = "items.jsonl"\n`, 'labels');
  write(`private/suites/${name}/items.jsonl`, items.map(item => JSON.stringify(item)).join('\n') + '\n', 'labels');
}
const knowledgeItems = [];
for (const [split, id, cause, action, limit] of knowledge) {
  const source = `public/sources/${id}.md`;
  const text = `# ${id}\n\nFictional operational handover. Valid at ${semanticTime}.\n\nObserved cause: ${cause}\n\nReviewed action: ${action}\n\nOperating policy: ${limit}\n\nNo measured financial impact or customer outage duration is recorded.\n`;
  write(source, text, 'source');
  seeds.push({ project: `r3-${id}`, summary: id, canonical_text: text,
    memory_type: 'reference', importance: 3, confidence: 0.9, tags: [id],
    authored_at: semanticTime, sources: [{ source_kind: 'note', excerpt: text }] });
  const prompts = [
    [`What caused the ${id} incident?`, cause, false],
    [`What is the reviewed corrective action for ${id}?`, action, false],
    [`What operating limit or interval applies to ${id}?`, limit, false],
    [`How many dollars did the ${id} incident cost customers?`, 'insufficient evidence', true],
  ];
  for (const [i, [question, expected, refuse]] of prompts.entries()) {
    const caseId = `${id}-${i+1}`;
    cases.push({ id: caseId, cluster: id, track: 'knowledge', split,
      suite: 'private/suites/knowledge', sources: [source] });
    knowledgeItems.push({ eval_type: 'grounded_answer', id: caseId, project: `r3-${id}`, question,
      top_k: 8, required_assertions: [expected], forbidden_assertions: [] });
    labels.push({ id: caseId, expected_facts: [expected], acceptable_alternatives: refuse
      ? ['The provided sources do not establish a financial loss.', 'Cannot determine from the evidence.']
      : ['A paraphrase preserving every operational qualifier and number.'],
      required_sources: refuse ? [] : [source], refusal_required: refuse,
      forbidden_claims: ['Invented costs, outage durations, or unrecorded causal claims.'],
      critical: !refuse, rubric_status: 'draft',
      note: 'Existing substring scorer is diagnostic only; citation/semantic rubric adjudication is required.' });
  }
}
suite('knowledge', knowledgeItems);

const codingItems = [];
for (const c of coding) {
  const source = `public/sources/${c.id}.md`;
  write(source, `# ${c.id}\n\nFictional maintenance contract.\n\n${c.contract}\n\nPublic signature: \`${c.signature}\`. No new dependencies. Retain behavior from earlier steps.\n`, 'source');
  const fixture = `public/fixtures/${c.id}`;
  write(`${fixture}/Cargo.toml`, `[package]\nname = "${c.id}"\nversion = "0.1.0"\nedition = "2024"\n\n[workspace]\n`, 'fixture');
  write(`${fixture}/src/lib.rs`, `${c.signature} { ${c.starter} }\n`, 'fixture');
  const steps = [];
  for (let step = 0; step < 3; step++) {
    // Grader runs in its own process/container after the agent exits; never mount
    // private/ in an agent. Include only returned implementation bytes to grade.
    const checks = c.checks.slice(0, step+1).flat().join('\n');
    write(`private/graders/${c.id}/step-${step+1}.rs`, `include!("submission.rs");\n#[test] fn acceptance() {\n${checks}\n}\n`, 'labels');
    steps.push({ id: `${c.id}-step-${step+1}`,
      prompt: `${['Repair nominal behavior', 'Handle documented boundary and empty-input cases', 'Harden extreme values and preserve all prior behavior'][step]} for ${c.id}. Consult the supplied contract. Do not change the public signature.`,
      memory_questions: [`What is the ${c.id} maintenance contract?`],
      score_commands: [`/opt/r3/grade ${c.id} ${step+1}`],
      required_files: ['src/lib.rs'], timeout_seconds: 120 });
  }
  codingItems.push({ eval_type: 'agent_build_sequence', id: c.id, project: `r3-${c.id}`,
    fixture: `../../../${fixture}`, agent_command: '/opt/r3/agent', timeout_seconds: 360, steps });
  cases.push({ id: c.id, cluster: c.id, track: 'coding', split: c.split,
    suite: 'private/suites/coding', sources: [source] });
  seeds.push({ project: `r3-${c.id}`, summary: c.id, canonical_text: c.contract,
    memory_type: 'reference', importance: 3, confidence: 0.9, tags: [c.id],
    authored_at: semanticTime, sources: [{source_kind: 'note', excerpt: c.contract}] });
  write(`private/reference-solutions/${c.id}.rs`, `${c.signature} { ${c.solution} }\n`, 'labels');
  labels.push({ id: c.id, expected_facts: [c.contract], required_sources: [source],
    hidden_tests: c.checks, steps: 3, scoring_unit: 'complete sequence',
    rubric_status: 'draft', limitation: 'Small single-function maintenance fixture, not large-repository evidence.' });
}
suite('coding', codingItems);
write('private/labels.json', labels, 'labels');
write('private/seed-records.json', seeds, 'configuration');
write('public/index.md', '# Handover source index\n\n' + cases.filter((c,i,a) => a.findIndex(x=>x.cluster===c.cluster)===i)
  .map(c => `- [${c.cluster}](sources/${c.cluster}.md)`).join('\n') + '\n', 'source');
write('configuration.json', { version: 1, semantic_time: semanticTime, service_read_only: true,
  reinforcement_ranking: true, learning: false, source_chunk_characters: 1024,
  context_bytes: 32768, top_k: 8, warmup: 'identical preparation outside timing',
  labels: 'draft', driver_status: 'integration pending; no paid execution authorized' }, 'configuration');
const manifest = { version: 1, name: 'r3-engineering-v1', semantic_time: semanticTime,
  seed: 20260910, repeats: 3, label_status: 'draft',
  model: { provider: 'pending', resolved_id: 'pending', reasoning: 'pending', embedding_id: 'pending',
    max_input_tokens: 32768, max_output_tokens: 8192,
    input_microusd_per_million: 0, output_microusd_per_million: 0 },
  limits: { requests: 10000, tokens: 100000000, microusd: 0, task_seconds: 600 },
  reference_baseline: null, assets, cases };
write('campaign.json', manifest);
write('REVIEW.md', `# R3 draft benchmark review\n\nNOT APPROVED. No official or paid runs authorized.\n\n` +
  `Review public/sources for factual sufficiency, private/labels.json for expected facts, alternative wording and citation/refusal policy, and private/graders for coding acceptance tests.\n\n` +
  `Counts: 40 development + 100 held-out knowledge questions; 4 development + 12 held-out coding sequences, three steps each. Source scenarios and repository identities do not cross splits.\n\n` +
  `The benchmark creator necessarily sees labels. Execution agents must never receive private/. Further development must use development cases only.\n\n` +
  `Knowledge questions include one unsupported financial-impact question per scenario. Coding problems are small contract-following exercises; they are not representative evidence of large-repository work. Decide whether this breadth is adequate before freezing labels.\n\n` +
  `Review decisions still required: acceptable paraphrases, citation requirements and suitability of coding difficulty. Existing substring checks are not a semantic correctness or refusal judge.\n\n` +
  `Do not mark reviewed automatically. After Olivier's review, update labels, regenerate asset hashes, freeze the manifest and obtain separate spending approvals. The private seed file is input material, not evidence that a service snapshot has been seeded or sealed.\n`);
console.log(JSON.stringify({ output: root, manifest_sha256: hash(JSON.stringify(manifest)),
  knowledge_questions: knowledgeItems.length, coding_sequences: codingItems.length, labels: 'draft' }, null, 2));
