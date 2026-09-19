// SPDX-License-Identifier: AGPL-3.0-or-later
// Content stays in the caller's isolated process; return metadata-only evidence.
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const fail = code => { throw Error(`Staging application probe failed: ${code}`); };

export function checkResults(value, allowedIds) {
  if (!Array.isArray(value?.results) || value.results.length === 0) fail('empty_results');
  if (value.results.some(row => !allowedIds.has(row.memory_id))) fail('cross_area_result');
  if (!Array.isArray(value.answer_citations)
    || value.answer_citations.some(row => !allowedIds.has(row.memory_id))) fail('cross_area_citation');
  if (value.answer_generation?.method === 'llm' || value.answer_generation?.token_usage) fail('provider_used');
  return { result_count: value.results.length, citation_count: value.answer_citations.length };
}

export async function probeApplication(input, request, cliQuery) {
  const { allowedIds, ownId, foreignIds, project, question, token, foreignToken } = input;
  if (!(allowedIds instanceof Set) || !allowedIds.size || [...allowedIds].some(id => !uuid.test(id))
    || !allowedIds.has(ownId) || !Array.isArray(foreignIds) || foreignIds.length !== 3
    || new Set(foreignIds).size !== 3 || foreignIds.some(id => !uuid.test(id) || allowedIds.has(id))
    || typeof project !== 'string' || !project || typeof question !== 'string' || !question
    || question.length > 4096 || typeof token !== 'string' || !token
    || typeof foreignToken !== 'string' || !foreignToken || token === foreignToken) fail('invalid_input');
  for (const credential of [null, 'invalid-disposable-token', foreignToken]) {
    const denied = await request('/v1/memory/' + ownId, { token: credential });
    if (denied.status !== 401) fail('authentication_not_denied');
  }
  const own = await request('/v1/memory/' + ownId, { token });
  if (own.status !== 200 || own.body?.id !== ownId) fail('own_record_unavailable');
  for (const id of foreignIds) {
    const denied = await request('/v1/memory/' + id, { token });
    if (![403, 404].includes(denied.status) || denied.body?.id || denied.body?.canonical_text) fail('foreign_record_visible');
  }
  const response = await request('/v1/query', { token, method: 'POST', body: {
    project, query: question, top_k: 8, min_confidence: 0, answer_mode: 'deterministic', retrieval_mode: 'lexical',
  } });
  if (response.status !== 200) fail('query_unavailable');
  const http = checkResults(response.body, allowedIds);
  const cli = checkResults(await cliQuery({ project, question }), allowedIds);
  const write = await request('/v1/capture/task', { token, method: 'POST', body: {
    project, prompt: 'Synthetic validation must not persist this request.',
  } });
  if (write.status !== 403) fail('read_only_write_not_denied');
  return { status: 'passed', own_record_visible: true, denied_auth_probes: 3,
    denied_foreign_areas: 3, read_only_write_denied: true, http, cli,
    disclosure: 'metadata_only_no_question_answer_or_source_text', model_answer_detected: false,
    no_egress_enforcement: 'must_be_verified_by_clone_orchestrator' };
}

export function loopbackRequest(base, fetcher = fetch) {
  const url = new URL(base);
  if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || !url.port
    || url.pathname !== '/' || url.search || url.hash || url.username || url.password) fail('nonlocal_endpoint');
  return async (path, { token, method = 'GET', body } = {}) => {
    if (!/^\/v1\/(?:memory\/[0-9a-f-]{36}|query|capture\/task)$/.test(path)) fail('unsupported_path');
    try {
      const response = await fetcher(new URL(path, url), { method, redirect: 'error',
        headers: { ...(token ? { authorization: `Bearer ${token}` } : {}), 'content-type': 'application/json' },
        body: body ? JSON.stringify(body) : undefined, signal: AbortSignal.timeout(15000) });
      const reader = response.body?.getReader(); if (!reader) fail('missing_body');
      const chunks = []; let bytes = 0;
      for (;;) { const part = await reader.read(); if (part.done) break;
        bytes += part.value.byteLength; if (bytes > 4 * 1024 * 1024) { await reader.cancel(); fail('oversized_response'); }
        chunks.push(part.value);
      }
      return { status: response.status, body: JSON.parse(Buffer.concat(chunks).toString('utf8')) };
    } catch { fail('bounded_http_request'); }
  };
}
