// SPDX-License-Identifier: AGPL-3.0-or-later
// Offline task execution for the existing campaign protocol. Synthetic excerpt
// and no-op agents exercise plumbing; they cannot measure model/product quality.
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { preparedContext, runOfflineKnowledge } from './knowledge.mjs';
import { runOfflineSequence } from './sequence.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const zeroUsage = () => ({ input_tokens: 0, output_tokens: 0, total_tokens: 0 });
const conditions = ['handover', 'lexical', 'vector', 'full-memory', 'no-memory'];
const excerptAgent = ['/usr/local/bin/node', '-e', `
  const fs=require('fs');const input=JSON.parse(fs.readFileSync('input.json','utf8'));
  const source=input.context[0];
  fs.writeFileSync('answer.json',JSON.stringify(source
    ? {text:source.text,refused:false,citations:[{source_id:source.id,quote:source.text}]}
    : {text:'Insufficient supplied evidence.',refused:true,citations:[]}));
`];
const noOpAgent = ['/usr/local/bin/node', '-e', `
  const fs=require('fs');const input=JSON.parse(fs.readFileSync('task.json','utf8'));
  if(!input.prompt || !Array.isArray(input.context)) process.exit(1);
`];

function runRetriever(binary, configuration, request, directory) {
  return new Promise((resolve, reject) => {
    const child = spawn(binary, [configuration], { cwd: directory,
      env: { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: directory }, stdio: ['pipe', 'pipe', 'pipe'] });
    let stdout = '';
    const timer = setTimeout(() => child.kill('SIGKILL'), 10000);
    child.stderr.resume();
    child.stdin.on('error', () => {});
    child.stdout.on('data', bytes => { stdout += bytes; if (Buffer.byteLength(stdout) > 2 * 1024 * 1024) child.kill('SIGKILL'); });
    child.on('error', () => { clearTimeout(timer); reject(Error('offline retriever unavailable')); });
    child.on('close', code => {
      clearTimeout(timer);
      try { if (code !== 0) throw Error('failed'); resolve(JSON.parse(stdout)); }
      catch { reject(Error('offline retriever failed')); }
    });
    child.stdin.end(JSON.stringify(request));
  });
}

// Fake vectors are deterministic and question-independent at preparation. They
// are explicitly not a real embedding model or equivalent to Memory ranking.
function fakeVector(text) { return [...createHash('sha256').update(text).digest().subarray(0, 16)].map(byte => (byte + 1) / 256); }

export async function createRehearsalExecutor(options) {
  const { root, manifestSha256, nodeImage, rustImage, scoreBinary, retrieverBinary,
    driverSha256, configurationSha256, serviceBuild } = structuredClone(options.configuration);
  const session = options.session; // Trusted host owner, never serialized to an agent.
  if (!path.isAbsolute(root) || !path.isAbsolute(scoreBinary) || !path.isAbsolute(retrieverBinary)
      || [manifestSha256, driverSha256, configurationSha256].some(value => !/^[a-f0-9]{64}$/.test(value))) throw Error('invalid rehearsal configuration');
  const manifestBytes = await fs.readFile(path.join(root, 'campaign.json'));
  const manifest = JSON.parse(manifestBytes);
  if (!manifest.smoke_only || manifest.limits.microusd !== 0 || manifest.label_status !== 'draft'
      || session.preparation.official_evidence !== false) throw Error('only draft zero-spend offline rehearsals are supported');
  const readAsset = async relative => {
    const asset = manifest.assets.find(asset => asset.path === relative);
    if (!asset || !relative.split('/').every(part => /^[a-zA-Z0-9_.-]+$/.test(part) && part !== '.' && part !== '..')) throw Error('unlocked rehearsal input');
    const file = path.join(root, relative);
    const stat = await fs.lstat(file);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 2 * 1024 * 1024) throw Error('invalid rehearsal asset');
    const bytes = await fs.readFile(file);
    if (hash(bytes) !== asset.sha256) throw Error('changed rehearsal input');
    return bytes.toString('utf8');
  };
  const verify = async () => {
    if (!(await fs.readFile(path.join(root, 'campaign.json'))).equals(manifestBytes)) throw Error('changed rehearsal manifest');
    for (const asset of manifest.assets) await readAsset(asset.path);
  };
  await verify();
  const families = JSON.parse(await readAsset('private/families.json'));
  let busy = false;
  return async work => {
    if (busy) throw Error('rehearsal executor busy');
    const selected = structuredClone(work);
    const item = manifest.cases.find(item => item.id === selected.case_id);
    if (!item || !conditions.includes(selected.condition) || !Number.isInteger(selected.repeat)
        || selected.repeat < 0 || selected.repeat >= 3) throw Error('unknown rehearsal work');
    busy = true;
    const started = performance.now();
    try {
      await verify();
      const family = families.find(family => family.id === item.cluster);
      if (!family || family.track !== item.track) throw Error('missing rehearsal family');
      const sources = await Promise.all(item.sources.map(async id => ({ id, text: await readAsset(id) })));
      const project = `r3-${item.cluster}`;
      const executed = await session.withInstance(project, async instance => {
        const contextFor = async question => {
          let retrieval;
          if (selected.condition === 'no-memory') retrieval = { results: [] };
          else if (selected.condition === 'full-memory') retrieval = await instance.query(question);
          else {
            const temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-task-retriever-'));
            try {
              const file = path.join(temporary, 'configuration.json');
              await fs.writeFile(file, JSON.stringify({ project, condition: 'external-retriever', mode: selected.condition,
                documents: sources, embeddings: sources.map(source => fakeVector(source.text)), top_k: 8, bytes: 32768 }), { flag: 'wx', mode: 0o600 });
              retrieval = await runRetriever(retrieverBinary, file, { schema_version: 1, project, condition: 'external-retriever',
                query: question, limit: 8, item_id: item.id, context: { fixture_path: '', hidden_facts: [] }, query_embedding: fakeVector(question) }, temporary);
            } finally { await fs.rm(temporary, { recursive: true }); }
          }
          return preparedContext({ condition: selected.condition, sources, retrieval, bindings: instance.bindings, project });
        };
        if (item.track === 'knowledge') {
          const rubric = JSON.parse(await readAsset(`private/rubrics/${item.id}.json`));
          const context = await contextFor(rubric.question);
          const answer = await runOfflineKnowledge({ image: nodeImage, scoreBinary, sources, context, question: rubric.question,
            rubric, manifestSha256, work: selected, argv: excerptAgent });
          return { success: answer.assessment.success, answer: answer.answer.text,
            knowledge_review: answer.evidence, unresolved_rubric: answer.assessment.unresolved,
            critical_failure: answer.assessment.critical_failure, diagnostics: [], input_sha256: answer.publicInputSha256 };
        }
        const stages = JSON.parse(await readAsset(family.stages));
        const initialFiles = {};
        for (const asset of manifest.assets.filter(asset => asset.path.startsWith(family.fixture + '/'))) {
          initialFiles[asset.path.slice(family.fixture.length + 1)] = await readAsset(asset.path);
        }
        const context = await contextFor(`What is the documented ${family.id} maintenance contract?`);
        // Each stage's prompt is selected in its own invocation; the sequence
        // runner supplies no private grader output or reference implementation.
        const sequence = await runOfflineSequence({ image: rustImage, initialFiles, submissionPaths: family.submission_paths,
          timeoutMs: 30000, steps: stages.map((stage, index) => ({ id: `${family.id}-${stage.id}`, argv: noOpAgent,
            inputFiles: { 'task.json': JSON.stringify({ prompt: stage.prompt, context }) },
            grader: { kind: 'rust-blackbox', probes: stages.slice(0, index + 1).flatMap(stage => stage.probes) } })) });
        return { success: sequence.success, answer: null, knowledge_review: null,
          unresolved_rubric: false, critical_failure: false, diagnostics: sequence.steps, input_sha256: sequence.initialSha256 };
      });
      await verify();
      const value = executed.value;
      const legacyCondition = selected.condition === 'vector' ? 'semantic' : selected.condition === 'handover' ? 'lexical' : selected.condition;
      const result = (id, success, duration, answer, sub_results = []) => ({ item_id: id,
        eval_type: item.track === 'knowledge' ? 'grounded_answer' : 'agent_build_sequence', condition: legacyCondition,
        metadata: { claim: 'offline-synthetic-not-quality', reasoning_mode: 'none' },
        success, skipped: false, scores: {}, duration_ms: Math.max(1, Math.ceil(duration)), token_usage: zeroUsage(), answer,
        notes: ['Offline synthetic agent; not measured model quality.', `Prepared input SHA-256: ${value.input_sha256}`,
          'Rehearsal timing includes disposable service setup and hidden grading; not model/user latency.'], sub_results });
      return { work: selected, model: manifest.model,
        result: result(item.id, value.success, performance.now() - started, value.answer,
          value.diagnostics.map(step => result(step.id, step.passed, step.agentElapsedMs + step.graderElapsedMs, null))),
        execution_microusd: 0, provider_requests: 0, uncertain_charge: false,
        critical_failure: value.critical_failure, unresolved_rubric: value.unresolved_rubric, knowledge_review: value.knowledge_review,
        isolation: { before_sha256: executed.beforeSha256, after_sha256: executed.afterSha256,
          snapshot_sha256: executed.beforeSha256, configuration_sha256: configurationSha256, driver_sha256: driverSha256,
          service_build: serviceBuild, semantic_time: manifest.semantic_time, sealed: true,
          restricted_network: true, hidden_tests_inaccessible: true } };
    } finally { busy = false; }
  };
}
