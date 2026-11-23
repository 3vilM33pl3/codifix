// SPDX-License-Identifier: AGPL-3.0-or-later
// Disposable native harness acceptance, never official R3/product-quality evidence.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';

const [sourceRoot, root] = process.argv.slice(2);
if (process.platform !== 'win32' || process.env.COMPUTERNAME !== 'MEMORY-WINLAB'
    || !/^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+$/.test(sourceRoot ?? '')
    || !/^C:\\MemoryLab\\runs\\[0-9a-f-]{36}\\native-eval$/.test(root ?? '')) {
  throw Error('Explicit disposable native evaluation paths required');
}
const load = name => import(pathToFileURL(path.join(sourceRoot, 'evals/r3/isolation', name)));
const { nativeProfileHash } = await load('native-windows.mjs');
const { runOfflineExecution } = await load('execution.mjs');
const { createKnowledgeRunner } = await load('knowledge.mjs');
const { createModelAgent } = await load('model-agent.mjs');
const bound = async file => ({ path: file, sha256: createHash('sha256').update(await fs.readFile(file)).digest('hex') });
const code = path.join(root, 'code');
const node = await bound(path.join(code, 'node.exe'));
const guest = await bound(path.join(code, 'native-guest.mjs'));
const scorer = await bound(path.join(code, 'r3_score.exe'));
const profile = { schema_version: 1, helper: await bound(path.join(root, 'evaluation-helper.exe')),
  io_root: path.join(root, 'io'), node, guest,
  toolchains: [{ root: code, files: [node, guest, scorer] }], search_path: [code], library_path: [] };
const execution = { kind: 'windows-native-v1', profile, profile_sha256: nativeProfileHash(profile) };
await fs.writeFile(path.join(root, 'profile.json'), JSON.stringify(profile), { flag: 'wx' });
const observations = [];
const run = async options => {
  const result = await runOfflineExecution({ execution, ...options });
  observations.push({ execution: result.execution, privateOperation: result.privateOperation,
    exitCode: result.exitCode, elapsedMs: result.elapsedMs });
  return result;
};
// Neither private marker is granted to the fresh answering AppContainer. No
// credentials or real source material exist anywhere in this proof.
const privateFile = path.join(root, 'other-project.txt');
const hidden = path.join(root, 'hidden-grader.txt');
await fs.writeFile(privateFile, 'OTHER_PROJECT_SYNTHETIC_ONLY', { flag: 'wx' });
await fs.writeFile(hidden, 'PRIVATE_RUBRIC_SYNTHETIC_ONLY', { flag: 'wx' });
const boundary = await run({ argv: [node.path, '-e', `
  const fs=require('node:fs');
  for (const file of ${JSON.stringify([privateFile, hidden])}) {
    try { fs.readFileSync(file); process.exit(31); }
    catch (error) { if (!['EACCES','EPERM'].includes(error.code)) throw error; }
  }
  if (['OPENAI_API_KEY','ANTHROPIC_API_KEY','GEMINI_API_KEY'].some(key=>process.env[key])) process.exit(32);
  process.stdout.write('private-boundaries-denied');
`], files: {}, exportPaths: [], timeoutMs: 10000 });
assert.equal(boundary.exitCode, 0);
assert.equal(boundary.stdout, 'private-boundaries-denied');
const sources = [{ id: 'public/source.md', text: 'History is retained.' },
  { id: 'private/unselected.md', text: 'UNSELECTED_SOURCE_SYNTHETIC_ONLY' }];
const options = { execution, scoreExecutable: scorer, sources, context: [sources[0]],
  question: 'What is retained?', manifestSha256: 'd'.repeat(64), timeoutMs: 10000,
  work: { case_id: 'native-knowledge', condition: 'lexical', repeat: 0 },
  rubric: { case_id: 'native-knowledge', question: 'What is retained?',
    criteria: ['factual_correctness','required_evidence','appropriate_refusal'].map((dimension, i) => ({
      id: `criterion-${i}`, dimension, instruction: 'PRIVATE_RUBRIC_SYNTHETIC_ONLY', critical: false })),
    minimum_citations: 1, citations_critical: true } };
const answerCommand = invalid => [node.path, '-e', `
  const fs=require('node:fs'); const raw=fs.readFileSync('input.json','utf8'), input=JSON.parse(raw);
  if (Object.keys(input).sort().join(',')!=='context,question'
      || raw.includes('UNSELECTED_SOURCE_SYNTHETIC_ONLY') || raw.includes('PRIVATE_RUBRIC_SYNTHETIC_ONLY')
      || input.context.length!==1) process.exit(33);
  const source=input.context[0];
  fs.writeFileSync('answer.json', JSON.stringify({text:source.text, refused:false,
    citations:[{source_id:source.id,quote:${invalid ? "'invented quotation'" : 'source.text'}}]}), {flag:'wx'});
`];
const answer = createKnowledgeRunner({ runExecution: run });
const positive = await answer({ ...options, argv: answerCommand(false) });
assert.equal(positive.assessment.citations_valid, true);
assert.equal(positive.assessment.unresolved, true);
assert.equal(positive.assessment.success, false, 'synthetic execution cannot auto-approve semantic labels');
const negative = await answer({ ...options, argv: answerCommand(true) });
assert.equal(negative.assessment.citations_valid, false);
assert.equal(negative.assessment.success, false);
assert.equal(negative.assessment.critical_failure, true);
assert.equal(positive.providerRequests + negative.providerRequests, 0);
let fakeCalls = 0;
const assertPrivatePayload = value => {
  if (typeof value === 'string') {
    assert(!value.includes(root), 'private host path entered model payload');
    assert(!value.includes('PRIVATE_RUBRIC_SYNTHETIC_ONLY'), 'hidden rubric entered model payload');
  } else if (value && typeof value === 'object') {
    for (const [key, child] of Object.entries(value)) {
      assertPrivatePayload(key);
      assertPrivatePayload(child);
    }
  }
};
// A Windows path must be checked as a string, not against JSON text whose
// escaped backslashes could hide a real leak from the assertion.
assert.throws(() => assertPrivatePayload({ nested: [{ path: root }] }), /private host path/);
assert.throws(() => assertPrivatePayload({ nested: ['PRIVATE_RUBRIC_SYNTHETIC_ONLY'] }), /hidden rubric/);
const agent = createModelAgent({ taskId: 'native-model-proof', nativeExecution: execution, runNative: run,
  model: { provider: 'synthetic', resolved_id: 'fake-native-proof', reasoning: 'none' },
  allowance: { input_tokens: 100, output_tokens: 100, microusd: 0 }, maxCalls: 2, taskTimeoutMs: 60000,
  invoke() { throw Error('A provider must never be invoked by this proof'); },
  mediator: { async dispatch(request) {
    fakeCalls++;
    assertPrivatePayload(request.payload);
    return { output: request.payload.previous ? { kind: 'finish' } : { kind: 'command', tool: 'node',
      args: ['-e', 'const fs=require("node:fs");const p="source.json";const v=JSON.parse(fs.readFileSync(p));v.count++;fs.writeFileSync(p,JSON.stringify(v));'] },
      usage: { input_tokens: 10, output_tokens: 5, microusd: 0 }, invoked: true };
  } } });
const modeled = await agent({ execution, files: { 'source.json': '{"count":0}' },
  argv: [node.path, '-e', 'process.exit(0)'], exportPaths: ['source.json'], timeoutMs: 20000, stepId: 'one-step' });
assert.equal(modeled.exitCode, 0);
assert.equal(JSON.parse(Buffer.from(modeled.files['source.json'], 'base64')).count, 1);
assert.deepEqual(modeled.commandExecutions.map(item => item.kind), ['model-command','final-export']);
assert.equal(fakeCalls, 2);
// These are expected *execution* failures, not successful benchmark answers.
// Inspect the helper receipt and trusted driver error so broken startup or
// uncertain cleanup cannot masquerade as an output/deadline-control pass.
for (const [kind, command, message] of [
  ['output', 'process.stdout.write("x".repeat(2*1024*1024))', 'native command output budget exceeded'],
  ['timeout', 'setTimeout(()=>{},60000)', 'native command deadline expired'],
]) {
  const before = new Set(await fs.readdir(profile.io_root));
  await assert.rejects(run({ argv: [node.path, '-e', command], files: {}, exportPaths: [], timeoutMs: 5000 }), /native evaluation failed/);
  const added = (await fs.readdir(profile.io_root)).filter(name => !before.has(name));
  assert.equal(added.length, 1);
  const resultRoot = path.join(profile.io_root, added[0], 'result');
  const receipt = JSON.parse(await fs.readFile(path.join(resultRoot, 'result.json')));
  assert.equal(receipt.state, 'failed');
  assert.equal(receipt.reason, 'execution_failed', `${kind} must not hide uncertain cleanup`);
  assert.equal(receipt.exit_code, 1);
  assert((await fs.readFile(path.join(resultRoot, 'stderr.bin'), 'utf8')).includes(message));
}
await fs.writeFile(path.join(root, 'result.json'), JSON.stringify({
  format: 'memory-native-evaluation-proof-v1', state: 'succeeded',
  profile_sha256: execution.profile_sha256, observations,
  private_project_denied: true, hidden_grader_denied: true, provider_environment_absent: true,
  knowledge_citations_verified: true, invented_citation_rejected: true,
  human_judgments_unresolved: true, provider_requests: 0, official_evidence: false,
  fake_model_callbacks: fakeCalls, native_model_command_and_final_export_verified: true,
  oversized_output_rejected: true, deadline_failure_reaped: true,
  rust_grading_verified: false, campaign_verified: false, disk_hard_limit_enforced: false,
}), { flag: 'wx' });
