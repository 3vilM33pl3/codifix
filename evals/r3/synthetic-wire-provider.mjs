// SPDX-License-Identifier: AGPL-3.0-or-later
// Disposable HTTP provider for exercising the complete campaign, not a model,
// tokenizer, price source or permission to send data outside this process.
import http from 'node:http';
import { once } from 'node:events';
import { createProviderTransport } from './provider-transport.mjs';
import { createProviderBindings, validateProviderProfile } from './provider-profile.mjs';
import { syntheticVector, syntheticEmbeddingModel } from './synthetic-embedding.mjs';

const noOp = ['/usr/local/bin/node', '-e',
  "const fs=require('fs');const input=JSON.parse(fs.readFileSync('task.json','utf8'));if(!input.prompt||!Array.isArray(input.context))process.exit(1);"];

export async function openSyntheticWireProvider(manifest) {
  manifest = structuredClone(manifest);
  const model = structuredClone(manifest.model);
  if (!manifest.smoke_only || manifest.label_status !== 'draft' || manifest.limits.microusd !== 0
      || !manifest.provider_budget || !manifest.preparation_budget || model.provider !== 'openai'
      || model.resolved_id !== 'synthetic-mediated-excerpt-noop-v1' || model.embedding_id !== syntheticEmbeddingModel
      || model.max_input_tokens < 64 || model.max_output_tokens < 1
      || model.input_microusd_per_million !== 0 || model.output_microusd_per_million !== 0
      || (model.cached_input_microusd_per_million ?? 0) !== 0) throw Error('synthetic zero-spend wire profile required');
  const { profile } = validateProviderProfile(manifest);
  if (profile.embedding.dimensions !== 16 || profile.embedding.input_microusd_per_million !== 0
      || [profile.responses_tokenizer, profile.embedding.tokenizer].some(counter => counter.id !== 'synthetic-wire-counter-not-real-tokenizer-v1')) {
    throw Error('synthetic zero-spend wire profile required');
  }
  let calls = 0, stopped = false;
  const shutdown = new AbortController();
  const server = http.createServer(async (req, res) => {
    try {
      if (stopped || req.method !== 'POST' || req.headers.authorization !== 'Bearer synthetic-wire-only'
          || !['/v1/responses', '/v1/embeddings'].includes(req.url)) throw Error('denied');
      const chunks = []; let size = 0;
      for await (const chunk of req) {
        if ((size += chunk.length) > 1024 * 1024) throw Error('oversized');
        chunks.push(chunk);
      }
      const body = JSON.parse(Buffer.concat(chunks));
      let reply;
      if (req.url === '/v1/embeddings') {
        if (body.model !== syntheticEmbeddingModel || body.dimensions !== 16 || body.encoding_format !== 'float'
            || !Array.isArray(body.input) || body.input.length !== 1) throw Error('invalid embedding');
        reply = { object: 'list', model: body.model,
          data: [{ object: 'embedding', index: 0, embedding: syntheticVector(body.input[0]) }],
          usage: { prompt_tokens: 1, total_tokens: 1 } };
      } else {
        if (body.model !== model.resolved_id || body.tools?.length !== 0 || body.store !== false
            || body.background !== false || body.stream !== false) throw Error('invalid answer');
        const input = JSON.parse(body.input[0].content[0].text);
        const schema = body.text.format.name;
        let output;
        if (schema === 'r3_grounded_answer_v1') {
          const source = input.context[0];
          output = source ? { text: source.text, refused: false, citations: [{ source_id: source.id, quote: source.text }] }
            : { text: 'Insufficient supplied evidence.', refused: true, citations: [] };
        } else if (schema === 'r3_isolated_action_v1') {
          output = { action: input.previous ? { kind: 'finish' } : { kind: 'command', argv: noOp } };
        } else throw Error('unknown schema');
        reply = { id: `resp_synthetic_${calls}`, object: 'response', status: 'completed', model: body.model,
          error: null, incomplete_details: null, output: [{ type: 'message', role: 'assistant', status: 'completed',
            content: [{ type: 'output_text', annotations: [], text: JSON.stringify(output) }] }],
          usage: { input_tokens: 1, output_tokens: 1, total_tokens: 2,
            input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 } } };
      }
      calls++;
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(reply));
    } catch { res.writeHead(400); res.end('invalid synthetic request'); }
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const transport = api => createProviderTransport({ api, offline: true, bearer: 'synthetic-wire-only',
    endpoint: `http://127.0.0.1:${server.address().port}/v1/${api}` });
  const cancellable = api => {
    const send = transport(api);
    return (body, signal) => send(body, AbortSignal.any([signal, shutdown.signal]));
  };
  const responses = cancellable('responses'), embeddings = cancellable('embeddings');
  return {
    get calls() { return calls; },
    bind(mediator, taskId) {
      if (stopped || typeof taskId !== 'string' || !/^(preparation|work-[0-9]{5})$/.test(taskId)) throw Error('invalid wire task identity');
      const counter = value => ({ id: value.id, sha256: value.asset.sha256, countInputTokens: () => 64 });
      return createProviderBindings({ manifest, mediator, taskId, responsesCounter: counter(profile.responses_tokenizer),
        embeddingCounter: counter(profile.embedding.tokenizer), sendResponses: responses, sendEmbeddings: embeddings, signal: shutdown.signal });
    },
    async close() {
      if (stopped) return;
      stopped = true; shutdown.abort(); server.closeAllConnections();
      await new Promise(resolve => server.close(resolve));
    },
  };
}
