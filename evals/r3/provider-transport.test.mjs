// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { once } from 'node:events';
import { createProviderTransport } from './provider-transport.mjs';

async function server(t, handler) {
  const instance = http.createServer(handler);
  instance.listen(0, '127.0.0.1');
  await once(instance, 'listening');
  t.after(async () => { instance.closeAllConnections(); await new Promise(resolve => instance.close(resolve)); });
  return `http://127.0.0.1:${instance.address().port}/v1/responses`;
}
const active = () => new AbortController().signal;

test('only explicit bounded JSON reaches the fixed loopback destination', async t => {
  let calls = 0;
  const endpoint = await server(t, async (req, res) => {
    calls++;
    assert.equal(req.url, '/v1/responses');
    assert.equal(req.method, 'POST');
    assert.equal(req.headers.authorization, 'Bearer synthetic-private-test');
    let body = '';
    for await (const chunk of req) body += chunk;
    assert.deepEqual(JSON.parse(body), { model: 'fake', input: 'question' });
    res.writeHead(200, { 'Content-Type': 'application/json; charset=utf-8' });
    res.end('{"ok":true}');
  });
  const send = createProviderTransport({ endpoint, bearer: 'synthetic-private-test' });
  assert.equal(await send({ model: 'fake', input: 'question' }, active()), '{"ok":true}');
  assert.equal(calls, 1);
  await assert.rejects(send({ input: 'x'.repeat(1024 * 1024) }, active()), /invalid provider request/);
  await assert.rejects(send({}, AbortSignal.abort()), /deadline signal/);
  assert.equal(calls, 1);
});

test('destination and credential selection never falls back to host configuration', () => {
  for (const endpoint of ['https://api.openai.com/v1/responses', 'http://localhost:8080/v1/responses',
    'http://127.0.0.1:8080/other', 'http://127.0.0.1:8080/v1/responses?key=secret',
    'http://user:password@127.0.0.1:8080/v1/responses', 'http://169.254.169.254/v1/responses']) {
    assert.throws(() => createProviderTransport({ endpoint, bearer: 'synthetic-test' }), /invalid provider/);
  }
  assert.throws(() => createProviderTransport({ endpoint: 'http://127.0.0.1:8080/v1/responses', bearer: 'non-synthetic' }));
  assert.throws(() => createProviderTransport({ endpoint: 'https://other.example/v1/responses', bearer: 'test', offline: false }));
});

test('redirects, error bodies, oversized streams and malformed encoding are not retried or exposed', async t => {
  let redirects = 0;
  const destination = await server(t, (_req, res) => { redirects++; res.end('SECRET'); });
  for (const kind of ['redirect', 'error', 'large', 'encoding', 'content-type']) {
    let calls = 0;
    const endpoint = await server(t, (_req, res) => {
      calls++;
      if (kind === 'redirect') { res.writeHead(307, { Location: destination }); res.end(); }
      else if (kind === 'error') { res.writeHead(429); res.end('SECRET PROVIDER BODY'); }
      else if (kind === 'content-type') { res.writeHead(200, { 'Content-Type': 'text/html' }); res.end('SECRET'); }
      else {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(kind === 'large' ? 'x'.repeat(1024 * 1024 + 1) : Buffer.from([0xff]));
      }
    });
    const send = createProviderTransport({ endpoint, bearer: 'synthetic-test' });
    await assert.rejects(send({}, active()), e => /uncertain attempt/.test(e.message) && !/SECRET/.test(e.message));
    assert.equal(calls, 1);
  }
  assert.equal(redirects, 0);
});

test('an aborted response remains uncertain with one dispatch', async t => {
  let calls = 0;
  const abort = new AbortController();
  const endpoint = await server(t, (_req, res) => {
    calls++;
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.write('{');
    setTimeout(() => abort.abort(), 10);
  });
  const send = createProviderTransport({ endpoint, bearer: 'synthetic-test' });
  await assert.rejects(send({}, abort.signal), /uncertain attempt/);
  assert.equal(calls, 1);
});
