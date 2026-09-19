import test from 'node:test';
import assert from 'node:assert/strict';
import { credential, metadata } from '../run-workshop-question-queue.mjs';

test('normal credential parser never falls back or prints the supplied value', () => {
  const synthetic = 'sk-SYNTHETIC_ONLY_NEVER_A_REAL_CREDENTIAL';
  assert.equal(credential(Buffer.from(`OPENAI_API_KEY="${synthetic}"\n`)), synthetic);
  for (const input of ['', 'OPENAI_API_KEY=bad', `OPENAI_API_KEY=${synthetic}\nOPENAI_API_KEY=${synthetic}`]) {
    assert.throws(() => credential(Buffer.from(input)), error => !error.message.includes(synthetic));
  }
});
test('worker journal emits only closed metadata and bounded UUIDs', () => {
  const privateText = 'SYNTHETIC_PRIVATE_BODY';
  const result = metadata(Buffer.from(JSON.stringify({outcome:'processed', body:privateText, work:{
    action_id:'11111111-1111-4111-8111-111111111111',attempt_id:'22222222-2222-4222-8222-222222222222',outcome:'completed',question:privateText,
  }})));
  assert(!JSON.stringify(result).includes(privateText));
  assert.equal(result.work_outcome,'completed');
  assert.throws(() => metadata(Buffer.from(JSON.stringify({outcome:privateText}))));
  assert.throws(() => metadata(Buffer.from(' '.repeat(16_385))));
});
