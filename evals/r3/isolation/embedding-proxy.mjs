// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted-host adapter for supplied vectors. This is not a provider/budget
// mediator: callers must account for preparation and execution separately.
import http from 'node:http';
import { randomBytes } from 'node:crypto';
import { once } from 'node:events';

export function checkedVector(value, dimensions) {
  if (!Array.isArray(value) || value.length !== dimensions
      || value.some(x => typeof x !== 'number' || !Number.isFinite(x))) throw Error('invalid supplied vector');
  const result = value.map(Math.fround);
  if (result.some(x => !Number.isFinite(x)) || !result.some(x => x !== 0)) throw Error('invalid supplied vector');
  return result;
}

export async function openEmbeddingProxy({ model, dimensions, embed }) {
  if (typeof model !== 'string' || !/^[A-Za-z0-9._-]{1,128}$/.test(model)
      || !Number.isInteger(dimensions) || dimensions < 1 || dimensions > 8192
      || typeof embed !== 'function') throw Error('invalid supplied embedding contract');
  const token = randomBytes(32).toString('hex');
  let permit, closed = false;
  const server = http.createServer(async (req, res) => {
    const current = permit;
    try {
      if (closed || !current || current.used || req.method !== 'POST' || req.url !== '/v1/embeddings'
          || req.headers.authorization !== `Bearer ${token}`) throw Error('denied');
      // Consume before parsing/callback: malformed or repeated requests cannot
      // turn a single authorized query into multiple model invocations.
      current.used = true;
      const chunks = [];
      let bytes = 0;
      for await (const chunk of req) {
        if ((bytes += chunk.length) > 8192) throw Error('oversized request');
        chunks.push(chunk);
      }
      const body = JSON.parse(Buffer.concat(chunks).toString('utf8'));
      if (Object.keys(body).sort().join(',') !== 'input,model' || body.model !== model
          || !Array.isArray(body.input) || body.input.length !== 1 || body.input[0] !== current.question) throw Error('denied');
      const vector = checkedVector(await embed(current.question, 'query'), dimensions);
      if (closed || permit !== current) throw Error('expired query');
      current.succeeded = true;
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ data: [{ index: 0, embedding: vector }] }));
    } catch {
      res.writeHead(403, { 'Content-Type': 'application/json' });
      res.end('{"error":"supplied embedding unavailable"}');
    }
  });
  server.requestTimeout = 10000;
  server.headersTimeout = 10000;
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return {
    model, dimensions, baseUrl: `http://127.0.0.1:${server.address().port}/v1`, token,
    async query(question, run) {
      if (closed || permit || typeof question !== 'string' || !question.trim()
          || Buffer.byteLength(question) > 4096) throw Error('supplied embedding query unavailable');
      const current = { question, used: false, succeeded: false };
      permit = current;
      try {
        const result = await run();
        if (!current.succeeded || result?.diagnostics?.semantic_status !== 'active_space_ok') throw Error('semantic retrieval was not verified');
        return result;
      } finally { permit = undefined; }
    },
    async close() {
      closed = true;
      permit = undefined;
      server.closeAllConnections();
      await new Promise(resolve => server.close(resolve));
    },
  };
}
