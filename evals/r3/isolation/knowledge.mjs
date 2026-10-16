// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted-host bridge: explicit retrieved chunks -> isolated offline answer ->
// existing Rust adjudicator. A trusted answer callback may supply a mediated
// tool-free answer; no private rubric/source pool is passed to that callback.
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { execFile, spawn } from 'node:child_process';
import { promisify, isDeepStrictEqual } from 'node:util';
import { runOfflineExecution, validateExecutionInput, validateExecutionOutput } from './execution.mjs';

const exec = promisify(execFile);
const hash = value => createHash('sha256').update(value).digest('hex');
const conditions = ['handover', 'lexical', 'vector', 'full-memory', 'no-memory'];
const safeSource = value => typeof value === 'string' && value.length <= 240
  && value.split('/').every(part => /^[a-zA-Z0-9_.#-]+$/.test(part) && part !== '.' && part !== '..');

function corpus(sources) {
  if (!Array.isArray(sources) || !sources.length || sources.length > 256) throw Error('invalid source corpus');
  const map = new Map();
  let bytes = 0;
  for (const source of sources) {
    if (!source || Object.keys(source).sort().join(',') !== 'id,text' || !safeSource(source.id)
        || typeof source.text !== 'string' || !source.text.length || map.has(source.id)
        || Buffer.byteLength(source.text) > 65536 || (bytes += Buffer.byteLength(source.text)) > 1048576) throw Error('invalid source corpus');
    map.set(source.id, source.text);
  }
  return map;
}

// Only the exact prepared chunk enters the answer process. Ranking selects IDs;
// response snippets, source paths, generated service answers and debug fields
// are never additional context. Official chunk/preparation parity is separate.
export function preparedContext({ condition, sources, retrieval, bindings = [], project, topK = 8, contextBytes = 32768 }) {
  const sourceMap = corpus(sources);
  if (!conditions.includes(condition) || !Number.isInteger(topK) || topK < 1 || topK > 8
      || !Number.isInteger(contextBytes) || contextBytes < 1 || contextBytes > 32768) throw Error('invalid context contract');
  if (!retrieval || !Array.isArray(retrieval.results) || retrieval.results.length > topK) throw Error('invalid retrieval results');
  if (condition === 'no-memory' && retrieval.results.length) throw Error('no-memory cannot receive retrieved context');
  const ids = new Set();
  const result = [];
  let bytes = 0;
  for (const hit of retrieval.results) {
    let id;
    if (condition === 'full-memory') {
      if (!Array.isArray(bindings) || typeof project !== 'string' || !project) throw Error('missing prepared Memory bindings');
      const matches = bindings.filter(binding => binding.memory_id === hit.memory_id);
      if (matches.length !== 1) throw Error('unbound Memory result');
      const binding = matches[0];
      id = binding.source;
      if (binding.project !== project || hit.project !== project || hit.canonical_id !== binding.canonical_id
          || hit.version_no !== binding.version_no || !sourceMap.has(id)
          || binding.source_sha256 !== hash(sourceMap.get(id))) throw Error('changed or cross-project Memory binding');
    } else {
      id = hit.id;
      if (!sourceMap.has(id) || hit.text !== sourceMap.get(id)
          || !Array.isArray(hit.citations) || hit.citations.length !== 1 || hit.citations[0] !== id) throw Error('changed baseline source');
    }
    if (ids.has(id)) throw Error('duplicate retrieved source');
    ids.add(id);
    // Match the fixed comparators: omit entire oversized chunks, never truncate.
    const text = sourceMap.get(id);
    if (bytes + Buffer.byteLength(text) <= contextBytes) {
      bytes += Buffer.byteLength(text);
      result.push({ id, text });
    }
  }
  return result;
}

function validateAnswer(answer) {
  if (!answer || Object.keys(answer).sort().join(',') !== 'citations,refused,text'
      || typeof answer.text !== 'string' || typeof answer.refused !== 'boolean'
      || !Array.isArray(answer.citations) || answer.citations.length > 8
      || Buffer.byteLength(JSON.stringify(answer)) > 65536) throw Error('invalid structured offline answer');
  for (const citation of answer.citations) {
    if (!citation || Object.keys(citation).sort().join(',') !== 'quote,source_id'
        || typeof citation.source_id !== 'string' || typeof citation.quote !== 'string') throw Error('invalid structured offline answer');
  }
}

const scoreInputLimit = 2 * 1024 * 1024;
const scoreOutputLimit = 4 * 1024 * 1024;

function nativeScorerBinding(execution, scorer) {
  if (!scorer || Object.keys(scorer).sort().join(',') !== 'path,sha256'
      || typeof scorer.path !== 'string' || !/^[A-Za-z]:[\\/]/.test(scorer.path)
      || scorer.path.slice(2).includes(':') || scorer.path.includes('\0')
      || scorer.path.split(/[\\/]/).slice(1).some(part => part === '.' || part === '..' || part.endsWith('.') || part.endsWith(' '))
      || path.win32.basename(scorer.path).toLowerCase() !== 'r3_score.exe'
      || !/^[a-f0-9]{64}$/.test(scorer.sha256)) throw Error('explicit pinned native knowledge scorer required; no Linux fallback');
  const matches = execution.profile?.toolchains?.flatMap(tree => tree.files ?? []).filter(file =>
    typeof file.path === 'string' && file.path.toLowerCase() === scorer.path.toLowerCase());
  if (matches?.length !== 1 || matches[0].sha256 !== scorer.sha256) throw Error('native knowledge scorer absent from reviewed toolchain');
}

// Trusted-host process, not the answering agent. The operator-installed immutable
// toolchain/ACL boundary supplies launch trust: checking one open file does not
// claim atomic same-handle CreateProcess. No rubric/source temporary files exist.
export function createNativeKnowledgeScorer({ fileSystem = fs, spawnProcess = spawn, platform = process.platform,
  systemRoot = process.env.SystemRoot ?? 'C:\\Windows' } = {}) {
  return async (scorer, input) => {
    if (platform !== 'win32') throw Error('native knowledge scorer requires Windows; no Linux fallback');
    const bytes = Buffer.from(JSON.stringify(input));
    if (bytes.length > scoreInputLimit || typeof systemRoot !== 'string' || !/^[A-Za-z]:\\Windows$/i.test(systemRoot)) throw Error('invalid native scoring input or system directory');
    const handle = await fileSystem.open(scorer.path, 'r');
    try {
      const stat = await handle.stat();
      if (!stat.isFile() || stat.size < 1 || stat.size > 512 * 1024 * 1024) throw Error('invalid native scorer file');
      const digest = createHash('sha256');
      let size = 0;
      for await (const chunk of handle.createReadStream({ autoClose: false })) {
        size += chunk.length;
        if (size > stat.size) throw Error('native scorer changed during read');
        digest.update(chunk);
      }
      if (size !== stat.size || digest.digest('hex') !== scorer.sha256) throw Error('native scorer bytes changed');
    } finally { await handle.close(); }
    return new Promise((resolve, reject) => {
      let child;
      try { child = spawnProcess(scorer.path, ['assess-stdin'], { cwd: path.win32.dirname(scorer.path),
        env: { SystemRoot: systemRoot, WINDIR: systemRoot, PATH: '' },
        shell: false, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] }); }
      catch { reject(Error('native knowledge adjudication process failed')); return; }
      let failure = false, total = 0;
      const stdout = [];
      const fail = () => { if (failure) return; failure = true; try { child.kill('SIGKILL'); } catch { /* close/error still owns cleanup */ } };
      const timer = setTimeout(fail, 10000);
      child.on('error', fail);
      child.stdin.on('error', fail); // Includes early exit/EPIPE; never print private input.
      const consume = (chunk, retain) => {
        total += chunk.length;
        if (total > scoreOutputLimit) fail();
        else if (retain) stdout.push(chunk);
      };
      child.stdout.on('data', chunk => consume(chunk, true));
      child.stderr.on('data', chunk => consume(chunk, false)); // Sanitized failure only.
      child.once('close', (code, signal) => {
        clearTimeout(timer);
        if (failure || signal || code !== 0) { reject(Error('native knowledge adjudication process failed')); return; }
        try { resolve(JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(stdout)))); }
        catch { reject(Error('invalid native knowledge assessment')); }
      });
      try { child.stdin.end(bytes); } catch { fail(); }
    });
  };
}
const nativeScorer = createNativeKnowledgeScorer();

async function runKnowledge(options, runExecution, scoreNative) {
  // Snapshot caller-owned inputs before the first await. Private sources/rubrics
  // are retained only on the host, not copied alongside the selected context.
  const { runAnswer, ...plain } = options;
  if (runAnswer !== undefined && typeof runAnswer !== 'function') throw Error('invalid trusted answer adapter');
  const { image, execution: backend, scoreExecutable, question, sources, context, argv, manifestSha256, work, rubric,
    scoreBinary, timeoutMs = 10000 } = structuredClone(plain);
  const native = backend?.kind === 'windows-native-v1';
  if (backend !== undefined && !native) throw Error('unknown knowledge execution backend; no Linux fallback');
  if (native) {
    if (runAnswer !== undefined || scoreBinary !== undefined || image !== undefined) throw Error('native knowledge accepts deterministic commands only; no Linux fallback');
    nativeScorerBinding(backend, scoreExecutable);
  } else if (scoreExecutable !== undefined) throw Error('native scorer cannot enter Linux adjudication');
  const sourceMap = corpus(sources);
  if (!Array.isArray(context) || context.length > 8 || !conditions.includes(work?.condition)
      || !/^[a-f0-9]{64}$/.test(manifestSha256) || (!native && !path.isAbsolute(scoreBinary))
      || typeof question !== 'string' || !question.length || Buffer.byteLength(question) > 4096
      || question !== rubric?.question || work.case_id !== rubric.case_id) throw Error('invalid knowledge execution contract');
  if (work.condition === 'no-memory' && context.length) throw Error('no-memory context is not empty');
  let contextBytes = 0;
  const seen = new Set();
  for (const source of context) {
    if (!source || Object.keys(source).sort().join(',') !== 'id,text' || !sourceMap.has(source.id)
        || source.text !== sourceMap.get(source.id) || seen.has(source.id)
        || (contextBytes += Buffer.byteLength(source.text)) > 32768) throw Error('context differs from prepared sources');
    seen.add(source.id);
  }
  const publicInput = JSON.stringify({ question, context });
  const started = performance.now();
  let answer, answerReceipt, providerRequests = 0, newProviderRequests = 0, executionMicrousd = 0, tokenUsage = { input_tokens: 0, output_tokens: 0 };
  if (runAnswer) {
    const received = await runAnswer(JSON.parse(publicInput));
    if (!received || !Number.isSafeInteger(received.providerRequests) || received.providerRequests < 0
        || !Number.isSafeInteger(received.newProviderRequests) || received.newProviderRequests < 0 || received.newProviderRequests > received.providerRequests
        || !Number.isSafeInteger(received.executionMicrousd) || received.executionMicrousd < 0
        || !received.tokenUsage || ['input_tokens', 'output_tokens'].some(k => !Number.isSafeInteger(received.tokenUsage[k]) || received.tokenUsage[k] < 0)) throw Error('missing mediated answer usage');
    ({ answer, providerRequests, newProviderRequests, executionMicrousd, tokenUsage } = structuredClone(received));
    validateAnswer(answer);
  } else {
    const invocation = { ...(native ? { execution: backend } : { image }), argv, files: { 'input.json': publicInput }, timeoutMs, exportPaths: ['answer.json'] };
    validateExecutionInput(invocation);
    const result = structuredClone(await runExecution(invocation));
    validateExecutionOutput(invocation, result);
    answerReceipt = result.execution;
    if (result.exitCode !== 0 || Object.keys(result.files).join(',') !== 'answer.json') throw Error('offline answer process failed');
    try {
      const bytes = Buffer.from(result.files['answer.json'], 'base64');
      if (bytes.length > 65536) throw Error('oversized');
      answer = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
      validateAnswer(answer);
    } catch { throw Error('invalid structured offline answer'); }
  }
  const answerDurationMs = performance.now() - started;
  const scoringStarted = performance.now();
  if (native) {
    const input = { manifest_sha256: manifestSha256, work, rubric, sources, answer };
    if (Buffer.byteLength(JSON.stringify(input)) > scoreInputLimit) throw Error('oversized native knowledge assessment');
    const scored = structuredClone(await scoreNative(structuredClone(scoreExecutable), structuredClone(input)));
    if (!scored || Object.keys(scored).sort().join(',') !== 'assessment,evidence'
        || !scored.assessment || scored.assessment.success !== false || scored.assessment.unresolved !== true
        || typeof scored.assessment.citations_valid !== 'boolean' || typeof scored.assessment.critical_failure !== 'boolean'
        || !/^[a-f0-9]{64}$/.test(scored.assessment.packet_sha256)
        || !isDeepStrictEqual(scored.evidence?.ballots, []) || !isDeepStrictEqual(scored.evidence?.history, [])
        || scored.evidence?.packet?.manifest_sha256 !== manifestSha256
        || !isDeepStrictEqual(scored.evidence?.packet?.rubric, rubric)
        || !isDeepStrictEqual(scored.evidence?.packet?.sources, sources)
        || !isDeepStrictEqual(scored.evidence?.packet?.answer, answer)) throw Error('invalid native knowledge assessment binding');
    return { version: 1, kind: 'native-windows-knowledge-smoke', officialEvidence: false,
      execution: answerReceipt, scorer: scoreExecutable, publicInputSha256: hash(publicInput), answerDurationMs,
      adjudicationPreparationMs: performance.now() - scoringStarted,
      providerRequests, newProviderRequests, executionMicrousd, tokenUsage,
      answer, evidence: scored.evidence, assessment: scored.assessment };
  }
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'memory-r3-knowledge-'));
  try {
    const input = path.join(directory, 'input.json');
    const out = path.join(directory, 'review');
    const ballots = path.join(directory, 'ballots.json');
    await fs.writeFile(input, JSON.stringify({ manifest_sha256: manifestSha256, work, rubric, sources, answer }), { flag: 'wx', mode: 0o600 });
    await fs.writeFile(ballots, '[]', { flag: 'wx', mode: 0o600 });
    const invoke = args => exec(scoreBinary, args, { cwd: directory, env: { PATH: '/usr/bin:/bin', HOME: directory },
      timeout: 10000, maxBuffer: 4 * 1024 * 1024, killSignal: 'SIGKILL' });
    await invoke(['prepare', input, out]);
    const scored = JSON.parse((await invoke(['score', path.join(out, 'private/evidence.json'), ballots])).stdout);
    return { version: 1, kind: runAnswer ? 'mediated-knowledge-answer' : 'offline-knowledge-answer', officialEvidence: false,
      image: runAnswer ? null : image, publicInputSha256: hash(publicInput), answerDurationMs,
      adjudicationPreparationMs: performance.now() - scoringStarted,
      providerRequests, newProviderRequests, executionMicrousd, tokenUsage,
      answer, evidence: scored.evidence, assessment: scored.assessment };
  } catch {
    throw Error('offline knowledge adjudication failed');
  } finally {
    await fs.rm(directory, { recursive: true });
  }
}

export function createKnowledgeRunner({ runExecution = runOfflineExecution, scoreNative = nativeScorer } = {}) {
  if (typeof runExecution !== 'function' || typeof scoreNative !== 'function') throw Error('invalid trusted knowledge execution adapter');
  return options => runKnowledge(options, runExecution, scoreNative);
}
export const runOfflineKnowledge = createKnowledgeRunner();
