// SPDX-License-Identifier: AGPL-3.0-or-later
import { lookup } from 'node:dns/promises';
import { request } from 'node:https';
import { BlockList, isIP } from 'node:net';

const denied = new BlockList();
for (const [address, prefix] of [['0.0.0.0',8],['10.0.0.0',8],['100.64.0.0',10],['127.0.0.0',8],['169.254.0.0',16],['172.16.0.0',12],['192.0.0.0',24],['192.0.2.0',24],['192.168.0.0',16],['198.18.0.0',15],['198.51.100.0',24],['203.0.113.0',24],['224.0.0.0',4],['240.0.0.0',4]]) denied.addSubnet(address,prefix,'ipv4');
for (const [address, prefix] of [['2001::',32],['2001:db8::',32],['2001:10::',28],['2001:20::',28],['2002::',16]]) denied.addSubnet(address,prefix,'ipv6');
const forbiddenTlds = new Set(['alt','arpa','example','internal','invalid','local','localhost','onion','test']);
export function publicAddress(address) {
  const family = isIP(address);
  if (family === 4) return !denied.check(address,'ipv4');
  if (family !== 6) return false;
  const first = Number.parseInt(address.split(':')[0],16);
  return first >= 0x2000 && first <= 0x3fff && !denied.check(address,'ipv6');
}
export function publicUrl(value) {
  const url = new URL(value);
  const host = url.hostname;
  if (url.protocol !== 'https:' || url.username || url.password || url.hash || (url.port && url.port !== '443') || isIP(host.replace(/^\[|\]$/g,'')) || !host.includes('.') || forbiddenTlds.has(host.split('.').at(-1)) || value.length > 4096) throw new Error('public HTTPS endpoint required');
  return url;
}
export function boundedJson(text) {
  const value = JSON.parse(text);
  const stack = [[value,0]];
  let count = 0;
  while (stack.length) {
    const [item,depth] = stack.pop();
    if (++count > 10000 || depth > 32 || (typeof item === 'string' && item.length > 32768)) throw new Error('JSON complexity limit');
    if (item && typeof item === 'object') {
      if (Object.keys(item).length > 512) throw new Error('JSON collection limit');
      for (const child of Object.values(item)) stack.push([child,depth+1]);
    }
  }
  return value;
}
/** Public-only, DNS-pinned HTTPS transport. Redirects are errors, never followed. */
export function createPublicFetch({ resolve = lookup, transport = request } = {}) {
  let active = 0;
  return async function publicFetch(input, init) {
    if (active >= 8) throw new Error('network concurrency limit');
    active++;
    let timer;
    try {
      const req = new Request(input, init);
      const url = publicUrl(req.url);
      const controller = new AbortController();
      timer = setTimeout(() => controller.abort(), 20000);
      const signal = AbortSignal.any([req.signal,controller.signal]);
      const addresses = await Promise.race([
        resolve(url.hostname,{all:true,verbatim:true}),
        new Promise((_,reject) => signal.addEventListener('abort',() => reject(new Error('network timeout')),{once:true})),
      ]);
      signal.throwIfAborted();
      if (!addresses.length || addresses.some(({address}) => !publicAddress(address))) throw new Error('non-public address denied');
      const body = req.body ? Buffer.from(await req.arrayBuffer()) : undefined;
      if (body && body.length > 262144) throw new Error('request too large');
      return await new Promise((resolveResponse,reject) => {
        const outgoing = transport(url,{
          method:req.method, headers:Object.fromEntries(req.headers), signal,
          lookup:(_hostname, options, callback) => options?.all
            ? callback(null,addresses)
            : callback(null,addresses[0].address,addresses[0].family),
        }, response => {
          if (response.statusCode >= 300 && response.statusCode < 400) { response.destroy(); reject(new Error('redirect denied')); return; }
          const chunks=[]; let length=0;
          response.on('data',chunk => { length+=chunk.length; if(length>262144) response.destroy(new Error('response too large')); else chunks.push(chunk); });
          response.on('error',reject);
          response.on('end',() => {
            try {
              const data=Buffer.concat(chunks);
              const contentType=String(response.headers['content-type'] || '');
              if(contentType.includes('json') && data.length) boundedJson(data.toString('utf8'));
              resolveResponse(new Response([204,205,304].includes(response.statusCode) ? null : data,{status:response.statusCode,headers:response.headers}));
            } catch(error) { reject(error); }
          });
        });
        outgoing.on('error',reject);
        outgoing.end(body);
      });
    } finally { clearTimeout(timer); active--; }
  };
}
