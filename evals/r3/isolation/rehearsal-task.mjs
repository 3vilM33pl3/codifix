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
import { bindCorpus, bindQuery, syntheticProfile } from '../embedding-evidence.mjs';
import { openRequestMediator } from '../request-mediator.mjs';
import { createModelAgent } from './model-agent.mjs';
import { runMediatedKnowledge } from './model-answer.mjs';
import { syntheticVector, syntheticEmbeddingModel } from '../synthetic-embedding.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const zeroUsage = () => ({ input_tokens: 0, output_tokens: 0, total_tokens: 0 });
const tokenUsage = usage => usage ? { input_tokens: usage.input_tokens, output_tokens: usage.output_tokens,
  total_tokens: usage.input_tokens + usage.output_tokens } : zeroUsage();
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

export async function createRehearsalExecutor(options) {
  const { root, manifestSha256, nodeImage, rustImage, scoreBinary, retrieverBinary,
    driverSha256, configurationSha256, serviceBuild, budgetBinary } = structuredClone(options.configuration);
  const session = options.session; // Trusted host owner, never serialized to an agent.
  const baselineEmbeddings = options.baselineEmbeddings;
  const wireProvider = options.wireProvider;
  if (!path.isAbsolute(root) || !path.isAbsolute(scoreBinary) || !path.isAbsolute(retrieverBinary)
      || [manifestSha256, driverSha256, configurationSha256].some(value => !/^[a-f0-9]{64}$/.test(value))) throw Error('invalid rehearsal configuration');
  const manifestBytes = await fs.readFile(path.join(root, 'campaign.json'));
  const manifest = JSON.parse(manifestBytes);
  if (!manifest.smoke_only || manifest.limits.microusd !== 0 || manifest.label_status !== 'draft'
      || session.preparation.official_evidence !== false) throw Error('only draft zero-spend offline rehearsals are supported');
  if (manifest.model.embedding_id !== syntheticEmbeddingModel) throw Error('synthetic embedding profile mismatch');
  if (!!manifest.provider_budget !== !!budgetBinary || (budgetBinary && !path.isAbsolute(budgetBinary))) throw Error('mediated rehearsal binary/profile mismatch');
  if (wireProvider && (!budgetBinary || typeof wireProvider.bind !== 'function')) throw Error('wire provider requires mediated execution');
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
  return async (work, providerContract) => {
    if (busy) throw Error('rehearsal executor busy');
    const selected = structuredClone(work);
    const item = manifest.cases.find(item => item.id === selected.case_id);
    if (!item || !conditions.includes(selected.condition) || !Number.isInteger(selected.repeat)
        || selected.repeat < 0 || selected.repeat >= 3) throw Error('unknown rehearsal work');
    busy = true;
    const started = performance.now();
    let mediator, observation;
    try {
      await verify();
      if (manifest.provider_budget) {
        const p = structuredClone(providerContract);
        if (!p || !/^work-[0-9]{5}$/.test(p.task_id) || p.directory !== path.join(root,'run',p.task_id,'provider')
            || p.manifest_sha256 !== manifestSha256 || p.limits?.microusd !== 0
            || !Number.isInteger(p.limits.requests) || p.limits.requests < 1 || p.limits.requests > manifest.provider_budget.max_calls_per_task
            || p.limits.tokens !== manifest.model.max_input_tokens + manifest.model.max_output_tokens
            || p.limits.task_seconds !== manifest.limits.task_seconds
            || JSON.stringify(p.limits) !== JSON.stringify(p.task_limits)) throw Error('invalid per-task provider contract');
        mediator = await openRequestMediator({ binary: budgetBinary, directory: p.directory,
          manifestSha256, limits: p.limits, taskLimits: p.task_limits });
      } else if (providerContract) throw Error('unexpected provider contract');
      const model = { provider: manifest.model.provider, resolved_id: manifest.model.resolved_id, reasoning: manifest.model.reasoning };
      const allowance = { input_tokens: 64, output_tokens: 64, microusd: 0 };
      // Symbolic fake usage exercises receipts, not a real tokenizer or billing.
      const fakeUsage = { input_tokens: 1, output_tokens: 1, microusd: 0 };
      const wire = wireProvider?.bind(mediator, providerContract.task_id);
      let embeddingCalls = 0;
      const queryEmbedding = async (input, purpose = 'query', signal) => {
        if (purpose !== 'query') throw Error('task cannot perform document preparation');
        if (wire) return wire.embed(input, purpose, signal);
        if (!mediator) return syntheticVector(input);
        return mediator.dispatch({ taskId: providerContract.task_id, requestId: `query-embedding-${embeddingCalls++}`,
          payload: { model: syntheticEmbeddingModel, purpose, input }, signal,
          allowance: { input_tokens: 64, output_tokens: 0, microusd: 0 }, timeoutMs: 10000,
          invoke: async payload => ({ output: syntheticVector(payload.input),
            usage: { input_tokens: 1, output_tokens: 0, microusd: 0 } }) });
      };
      const family = families.find(family => family.id === item.cluster);
      if (!family || family.track !== item.track) throw Error('missing rehearsal family');
      const sources = await Promise.all(item.sources.map(async id => ({ id, text: await readAsset(id) })));
      const project = `r3-${item.cluster}`;
      const executed = await session.withInstance(project, async instance => {
        let retrievalMs = 0;
        const contextFor = async question => {
          let retrieval, retrievalStarted = performance.now();
          if (selected.condition === 'no-memory') retrieval = { results: [] };
          else if (selected.condition === 'full-memory') retrieval = await instance.query(question);
          else {
            const temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-task-retriever-'));
            try {
              const file = path.join(temporary, 'configuration.json');
              const profile = syntheticProfile(16, syntheticEmbeddingModel);
              const vectors = selected.condition === 'vector' ? sources.map(source => {
                if (!baselineEmbeddings) return syntheticVector(source.text);
                const vector = baselineEmbeddings.get(`${project}:${source.id}`);
                if (!vector) throw Error('missing prepared baseline vector');
                return vector;
              }) : [];
              await fs.writeFile(file, JSON.stringify({ project, condition: 'external-retriever', mode: selected.condition,
                documents: sources, ...(selected.condition === 'vector'
                  ? { embeddings: vectors, embedding_corpus: bindCorpus(profile, sources, vectors) } : {}),
                top_k: 8, bytes: 32768 }), { flag: 'wx', mode: 0o600 });
              retrievalStarted = performance.now(); // Prepared corpus/configuration is not task retrieval.
              // Query embedding is execution, not corpus preparation. Plain
              // document/lexical conditions must not invoke an embedding model.
              const queryVector = selected.condition === 'vector' ? await queryEmbedding(question) : undefined;
              retrieval = await runRetriever(retrieverBinary, file, { schema_version: 1, project, condition: 'external-retriever',
                query: question, limit: 8, item_id: item.id, context: { fixture_path: '', hidden_facts: [] },
                ...(queryVector ? { query_embedding: queryVector, embedding_query: bindQuery(profile, question, queryVector) } : {}) }, temporary);
            } finally { await fs.rm(temporary, { recursive: true }); }
          }
          const context = preparedContext({ condition: selected.condition, sources, retrieval, bindings: instance.bindings, project });
          retrievalMs += performance.now() - retrievalStarted;
          return context;
        };
        if (item.track === 'knowledge') {
          const rubric = JSON.parse(await readAsset(`private/rubrics/${item.id}.json`));
          const context = await contextFor(rubric.question);
          const answerOptions = { image: nodeImage, scoreBinary, sources, context, question: rubric.question,
            rubric, manifestSha256, work: selected, argv: excerptAgent };
          const answer = mediator ? await runMediatedKnowledge({ ...answerOptions, mediator, model, allowance,
            taskId: providerContract.task_id, requestId: 'answer', ...(wire ? { prepareRequest: wire.prepareRequest } : { invoke: async payload => {
              const source = payload.input.context[0];
              return { output: source ? { text: source.text, refused: false, citations: [{ source_id: source.id, quote: source.text }] }
                : { text: 'Insufficient supplied evidence.', refused: true, citations: [] }, usage: fakeUsage };
            } }) }) : await runOfflineKnowledge(answerOptions);
          return { success: answer.assessment.success, answer: answer.answer.text,
            knowledge_review: answer.evidence, unresolved_rubric: answer.assessment.unresolved,
            critical_failure: answer.assessment.critical_failure, diagnostics: [], input_sha256: answer.publicInputSha256,
            providerUsage: { provider_requests: answer.providerRequests, ...answer.tokenUsage, microusd: answer.executionMicrousd },
            timing: { retrieval_ms: Math.ceil(retrievalMs), answer_or_agent_ms: Math.max(1,Math.ceil(answer.answerDurationMs)),
              grading_ms: Math.ceil(answer.adjudicationPreparationMs) } };
        }
        const stages = JSON.parse(await readAsset(family.stages));
        const initialFiles = {};
        for (const asset of manifest.assets.filter(asset => asset.path.startsWith(family.fixture + '/'))) {
          initialFiles[asset.path.slice(family.fixture.length + 1)] = await readAsset(asset.path);
        }
        const context = await contextFor(`What is the documented ${family.id} maintenance contract?`);
        // Each stage's prompt is selected in its own invocation; the sequence
        // runner supplies no private grader output or reference implementation.
        const runAgent = mediator ? createModelAgent({ mediator, model, allowance, taskId: providerContract.task_id,
          maxCalls: manifest.provider_budget.max_calls_per_task, taskTimeoutMs: manifest.limits.task_seconds * 1000,
          ...(wire ? { prepareRequest: wire.prepareRequest } : {
            invoke: async payload => ({ output: payload.previous ? { kind: 'finish' } : { kind: 'command', argv: noOpAgent }, usage: fakeUsage }) }) }) : undefined;
        const sequence = await runOfflineSequence({ image: rustImage, initialFiles, submissionPaths: family.submission_paths, runAgent,
          timeoutMs: 30000, steps: stages.map((stage, index) => ({ id: `${family.id}-${stage.id}`, argv: noOpAgent,
            inputFiles: { 'task.json': JSON.stringify({ prompt: stage.prompt, context }) },
            grader: { kind: 'rust-blackbox', probes: stages.slice(0, index + 1).flatMap(stage => stage.probes) } })) });
        return { success: sequence.success, answer: null, knowledge_review: null,
          unresolved_rubric: false, critical_failure: false, diagnostics: sequence.steps, input_sha256: sequence.initialSha256,
          providerUsage: sequence.providerUsage,
          timing: { retrieval_ms: Math.ceil(retrievalMs), answer_or_agent_ms: sequence.steps.reduce((sum,step) => sum + Math.ceil(step.agentElapsedMs),0),
            grading_ms: sequence.steps.reduce((sum,step) => sum + Math.ceil(step.graderElapsedMs),0) } };
      }, { queryEmbedding });
      await verify();
      const value = executed.value;
      // Task totals include retrieval model calls once. Sequence step totals
      // remain agent-only diagnostics, not an additional execution charge.
      const audit = mediator ? (await mediator.audit()).audit : undefined;
      if (audit) {
        const totals = audit.tasks[providerContract.task_id];
        if (!totals) throw Error('missing task provider totals');
        value.providerUsage = { provider_requests: totals.requests, input_tokens: totals.input_tokens,
          output_tokens: totals.output_tokens, microusd: totals.microusd };
      }
      const legacyCondition = selected.condition === 'vector' ? 'semantic' : selected.condition === 'handover' ? 'lexical' : selected.condition;
      const result = (id, success, duration, answer, sub_results = [], usage) => ({ item_id: id,
        eval_type: item.track === 'knowledge' ? 'grounded_answer' : 'agent_build_sequence', condition: legacyCondition,
        metadata: { claim: 'offline-synthetic-not-quality', reasoning_mode: 'none' },
        success, skipped: false, scores: {}, duration_ms: Math.max(1, Math.ceil(duration)), token_usage: tokenUsage(usage), answer,
        notes: ['Offline synthetic agent; not measured model quality.', `Prepared input SHA-256: ${value.input_sha256}`,
          'Execution timing excludes disposable service setup and private grading; simulated callbacks are not model latency evidence.'], sub_results });
      return observation = { work: selected, model: manifest.model,
        timing: { version: 1, ...value.timing, executor_total_ms: 0 },
        result: result(item.id, value.success, value.timing.retrieval_ms + value.timing.answer_or_agent_ms, value.answer,
          value.diagnostics.map(step => ({ id: step.id, eval_type: 'agent_build_step',
            metadata: { claim: 'offline-synthetic-not-quality', reasoning_mode: 'none', grading_duration_ms: String(Math.ceil(step.graderElapsedMs)) },
            success: step.passed, skipped: false, scores: {},
            duration_ms: Math.max(1, Math.ceil(step.agentElapsedMs)), token_usage: tokenUsage(step.providerUsage),
            notes: [`Agent input SHA-256: ${step.agentInputSha256}`, `Private grader input SHA-256: ${step.graderInputSha256}`] })),value.providerUsage),
        execution_microusd: value.providerUsage?.microusd ?? 0, provider_requests: value.providerUsage?.provider_requests ?? 0, uncertain_charge: false,
        ...(audit ? { provider_audit: audit } : {}),
        critical_failure: value.critical_failure, unresolved_rubric: value.unresolved_rubric, knowledge_review: value.knowledge_review,
        isolation: { before_sha256: executed.beforeSha256, after_sha256: executed.afterSha256,
          snapshot_sha256: executed.beforeSha256, configuration_sha256: configurationSha256, driver_sha256: driverSha256,
          service_build: serviceBuild, semantic_time: manifest.semantic_time, sealed: true,
          restricted_network: true, hidden_tests_inaccessible: true } };
    } finally { try { await mediator?.close(); } finally {
      if (observation) observation.timing.executor_total_ms = Math.ceil(performance.now() - started);
      busy = false;
    } }
  };
}
