#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Hash-locked development rehearsal, not the complete/approved R3 benchmark.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const here = path.dirname(fileURLToPath(import.meta.url));
const [destination, selection, ...extra] = process.argv.slice(2);
if (!destination || extra.length || (selection !== undefined && !['--one-family-per-track', '--held-out'].includes(selection))) {
  throw Error('usage: node build-representative-pack.mjs NEW_DIRECTORY [--one-family-per-track | --held-out]');
}
// Held-out material is never silently copied into a development/calibration pack.
const split = selection === '--held-out' ? 'held_out' : 'development';
const splitDirectory = split.replace('_', '-');
const minimal = selection === '--one-family-per-track';
const root = path.resolve(destination);
// Exclusive creation, including an existing empty directory: never overwrite a
// reviewed pack or silently merge labels from a previous experiment.
fs.mkdirSync(root, { mode: 0o700 });
const hash = value => createHash('sha256').update(value).digest('hex');
const assets = [], cases = [], knowledgeItems = [], codingItems = [], families = [];
const semanticTime = '2026-09-10T00:00:00Z';
function write(relative, content, kind) {
  const bytes = typeof content === 'string' ? content : JSON.stringify(content, null, 2) + '\n';
  const target = path.join(root, relative);
  fs.mkdirSync(path.dirname(target), { recursive: true, mode: 0o700 });
  fs.writeFileSync(target, bytes, { flag: 'wx', mode: 0o600 });
  if (kind) assets.push({ path: relative, sha256: hash(bytes), kind });
}
function read(family, relative) {
  const file = path.join(here, 'representative', splitDirectory, family, relative);
  const stat = fs.lstatSync(file);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 1048576) throw Error('invalid representative input');
  return fs.readFileSync(file, 'utf8');
}
function files(family, relative) {
  const directory = path.join(here, 'representative', splitDirectory, family, relative);
  return fs.readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name < b.name ? -1 : a.name > b.name ? 1 : 0).flatMap(entry => {
    if (entry.name === 'target' || entry.name === '.git') return [];
    if (entry.isSymbolicLink()) throw Error('symlink in representative fixture');
    const name = `${relative}/${entry.name}`;
    if (entry.isDirectory()) return files(family, name);
    if (!entry.isFile()) throw Error('special file in representative fixture');
    return [name];
  });
}
const knowledge = split === 'held_out' ? ['telemetry-units']
  : ['supplier-handover', 'turnover-recovery', 'cache-partition', 'schema-transition', 'capacity-pressure', 'timezone-window', 'artifact-origin', 'retention-hold', 'quota-accounting', 'dns-cutover'];
const coding = split === 'held_out' ? ['framed-stream', 'interval-calendar']
  : ['durable-importer', 'evidence-publication', 'configuration-rollout', 'dependency-build'];
for (const family of minimal ? knowledge.slice(0, 1) : knowledge) {
  const sources = files(family, 'public').map(relative => {
    const target = `public/sources/${family}/${relative.slice('public/'.length)}`;
    write(target, read(family, relative), 'source');
    return target;
  });
  write(`private/reviews/${family}.md`, read(family, 'review.md'), 'labels');
  const ids = [];
  for (let i = 1; i <= 4; i++) {
    const bytes = read(family, `private/q${i}.json`);
    const rubric = JSON.parse(bytes);
    if (rubric.case_id !== `${family}-q${i}`) throw Error('unexpected rubric identity');
    const label = `private/rubrics/${rubric.case_id}.json`;
    write(label, bytes, 'labels'); // Preserve canonical Rust rubric bytes exactly.
    const id = rubric.case_id;
    ids.push(id);
    cases.push({ id, cluster: family, track: 'knowledge', split, suite: 'private/suites/knowledge', sources });
    knowledgeItems.push({ eval_type: 'grounded_answer', id, project: `r3-${family}`, question: rubric.question,
      top_k: 8, required_assertions: [], forbidden_assertions: [] });
  }
  families.push({ id: family, track: 'knowledge', split, cases: ids, sources });
}
for (const family of minimal ? coding.slice(0, 1) : coding) {
  const fixture = `public/fixtures/${family}`;
  const source = `public/sources/${family}/contract.md`;
  write(source, read(family, 'public/contract.md'), 'source');
  const publicFiles = files(family, 'public');
  for (const relative of publicFiles) write(`${fixture}/${relative.slice('public/'.length)}`, read(family, relative), 'fixture');
  for (const relative of files(family, 'private')) write(`private/graders/${family}/${relative.slice('private/'.length)}`, read(family, relative), 'labels');
  write(`private/reviews/${family}.md`, read(family, 'review.md'), 'labels');
  const stages = JSON.parse(read(family, 'private/steps.json'));
  if (stages.length !== 3) throw Error('coding requires three stages');
  const sourceFiles = publicFiles.filter(file => file.startsWith('public/src/')).map(file => file.slice('public/'.length));
  codingItems.push({ eval_type: 'agent_build_sequence', id: family, project: `r3-${family}`, fixture: `../../../${fixture}`,
    agent_command: '/opt/r3/agent', timeout_seconds: 360,
    steps: stages.map(stage => ({ id: `${family}-${stage.id}`, prompt: stage.prompt,
      memory_questions: [`What is the documented ${family} maintenance contract?`],
      score_commands: [`/opt/r3/grade ${family} ${stage.id}`], required_files: sourceFiles, timeout_seconds: 120 })) });
  cases.push({ id: family, cluster: family, track: 'coding', split, suite: 'private/suites/coding', sources: [source] });
  families.push({ id: family, track: 'coding', split, cases: [family], sources: [source],
    fixture, submission_paths: sourceFiles, stages: `private/graders/${family}/steps.json` });
}
for (const [name, items] of [['knowledge', knowledgeItems], ['coding', codingItems]]) {
  write(`private/suites/${name}/suite.toml`, `name = "r3-representative-${name}"\nsuite_version = "r3-draft-1"\nlabel_status = "draft"\nproject = "r3-${name}"\ndefault_profile = "offline"\ndefault_repeats = 3\nitems = "items.jsonl"\n`, 'labels');
  write(`private/suites/${name}/items.jsonl`, items.map(item => JSON.stringify(item)).join('\n') + '\n', 'labels');
}
write('private/families.json', families, 'configuration');
write('configuration.json', { version: 1, semantic_time: semanticTime, labels: 'draft', offline_only: true,
  context_bytes: 32768, top_k: 8, learning: false, reinforcement_ranking: true,
  driver_status: 'offline rehearsal available; provider-backed execution pending', embedding_status: 'offline fixtures only' }, 'configuration');
const manifest = { version: 1, name: `r3-representative-${split}-rehearsal`, smoke_only: true,
  semantic_time: semanticTime, seed: 20260910, repeats: 3, label_status: 'draft',
  model: { provider: 'offline-fake', resolved_id: 'deterministic-test-agent-v1', reasoning: 'none', embedding_id: 'offline-fake-only',
    max_input_tokens: 32768, max_output_tokens: 8192, input_microusd_per_million: 0, output_microusd_per_million: 0 },
  limits: { requests: 1000, tokens: 100000000, microusd: 0, task_seconds: 600 }, reference_baseline: null, assets, cases };
write('campaign.json', manifest);
write('REVIEW.md', `# Representative ${split} fixture pack — DRAFT\n\n` +
  `This partial pack has ${knowledgeItems.length} knowledge questions and ${codingItems.length} three-step coding sequences. ` +
  `It contains only ${split} cases and is smoke-only: no label approval, spend approval or product-quality evidence. ` +
  `Held-out fixture checks use deterministic test doubles only, never development model calibration. ` +
  `The official 40/100 knowledge and 4/12 coding sizes are unchanged.\n\n` +
  `Review public source facts and private/reviews, rubrics and graders. Only public fixture files and condition-selected source chunks may enter an agent. ` +
  `Private labels, reference implementations and review material must stay on the host. Source versions and corpus preparation must still be bound by the driver.\n\n` +
  `All informed conditions receive the same underlying source facts. The standalone suites retain compatibility with mem-eval, but their placeholder agent/grader commands are not an executable driver. ` +
  `No paid evaluation is enabled by this generator.\n`);
console.log(JSON.stringify({ output: root, knowledge_questions: knowledgeItems.length, coding_sequences: codingItems.length,
  labels: 'draft', official_evidence: false }));
