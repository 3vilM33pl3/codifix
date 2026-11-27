// SPDX-License-Identifier: AGPL-3.0-or-later
import { createServer } from 'node:http';
import { lstat, chmod, unlink } from 'node:fs/promises';
import { dirname, isAbsolute } from 'node:path';
import { boundedJson } from './network.mjs';

async function readBody(req) {
  if(req.headers['content-type']?.split(';')[0]!=='application/json') throw new Error('JSON required');
  const chunks=[]; let length=0;
  for await(const chunk of req) { length+=chunk.length; if(length>8192) throw new Error('request too large'); chunks.push(chunk); }
  return boundedJson(Buffer.concat(chunks).toString('utf8'));
}
export function createHandler(broker) {
  let active=0;
  return async(req,res)=>{
    res.setHeader('cache-control','no-store'); res.setHeader('content-type','application/json'); res.setHeader('x-content-type-options','nosniff');
    if(active>=8) { res.writeHead(429); res.end('{"error":"identity_busy"}'); return; }
    active++;
    try {
      if(!req.url || req.url.length>8192) throw new Error('invalid request');
      const url=new URL(req.url,'http://identity.local'); let result;
      if(req.method==='GET' && url.pathname==='/oauth/client-metadata.json' && !url.search) result=broker.client.clientMetadata;
      else if(req.method==='GET' && url.pathname==='/oauth/callback') result=await broker.callback(url.searchParams);
      else if(req.method==='POST' && !url.search) {
        const body=await readBody(req);
        if(url.pathname==='/internal/link/begin') result=await broker.begin(body.binding,body.login_hint);
        else if(url.pathname==='/internal/link/poll') result=await broker.poll(body.binding);
        else if(url.pathname==='/internal/link/ack') result=await broker.acknowledge(body.binding);
        else if(url.pathname==='/internal/discover') result={profiles:await broker.discover(body.identifier)};
        else { res.writeHead(404); res.end('{"error":"not_found"}'); return; }
      } else { res.writeHead(404); res.end('{"error":"not_found"}'); return; }
      res.end(JSON.stringify(result));
    } catch { res.writeHead(400); res.end('{"error":"identity_request_rejected"}'); }
    finally { active--; }
  };
}
export async function listenProtected(socketPath,broker) {
  if(!isAbsolute(socketPath)) throw new Error('absolute socket path required');
  const parent=await lstat(dirname(socketPath));
  if(!parent.isDirectory() || parent.isSymbolicLink() || (parent.mode & 0o022)!==0 || parent.uid!==process.getuid()) throw new Error('socket directory must be broker-owned and not group/world writable');
  try { await lstat(socketPath); throw new Error('socket path already exists'); } catch(error) { if(error.code!=='ENOENT') throw error; }
  const server=createServer({requestTimeout:30000,headersTimeout:10000,maxHeaderSize:12288},createHandler(broker));
  // Restrict the socket at creation, before listen's callback can run.
  const previous=process.umask(0o117);
  try { await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(socketPath,resolve);}); }
  finally { process.umask(previous); }
  await chmod(socketPath,0o660);
  const original=await lstat(socketPath);
  const close=async()=>{
    await new Promise(resolve=>server.close(resolve));
    try {const current=await lstat(socketPath);if(current.ino===original.ino && current.dev===original.dev) await unlink(socketPath);} catch(error) {if(error.code!=='ENOENT') throw error;}
  };
  return {server,close};
}
