// SPDX-License-Identifier: AGPL-3.0-or-later
// Optional trusted preparation happens before the durable reservation. Keep its
// time inside the task deadline; it cannot replace the task/request identities.
export async function dispatchModel({ mediator, prepareRequest, invoke, ...request }) {
  if (!prepareRequest) return mediator.dispatch({ ...request, invoke });
  if (invoke !== undefined || typeof prepareRequest !== 'function') throw Error('choose one model dispatch adapter');
  const started = performance.now();
  const prepared = await prepareRequest(request.payload);
  const remaining = Math.floor(request.timeoutMs - (performance.now() - started));
  if (!prepared || Object.keys(prepared).sort().join(',') !== 'allowance,decodeStored,invoke,payload'
      || typeof prepared.invoke !== 'function' || typeof prepared.decodeStored !== 'function'
      || remaining < 1) throw Error('invalid or expired prepared model request');
  const result = await mediator.dispatch({ ...request, payload: prepared.payload, allowance: prepared.allowance,
    timeoutMs: remaining, invoke: prepared.invoke, withReceipt: true });
  return { ...result, output: prepared.decodeStored(result.output) };
}
