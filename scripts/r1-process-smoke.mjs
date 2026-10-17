// SPDX-License-Identifier: AGPL-3.0-or-later
// Real-process fault proof. Owns a new disposable container, files and services.
// No provider credentials, customer database or installed service is used.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import http from 'node:http';
import net from 'node:net';

const binary = resolve(process.argv[2] ?? 'target/debug/codifix');
const directory = mkdtempSync(join(tmpdir(), 'memory-r1-process-'));
const container = `memory-r1-process-${randomUUID()}`;
const image = 'pgvector/pgvector@sha256:cf134a767f474095eeba57e0117be8e568e011a63f33fbf252f14c9b760f8e6f';
const token = 'disposable-process-test';
const children = [];
const calls = [];
let holdNext = false;
let started = false;
let interrupted = false;
for (const signal of ['SIGINT','SIGTERM']) process.once(signal, () => { interrupted = true; });
const provider = http.createServer(async (request, response) => {
  const parts = [];
  for await (const part of request) parts.push(part);
  const body = JSON.parse(Buffer.concat(parts).toString());
  calls.push(body);
  if (holdNext) { holdNext = false; return; } // killed owner's request remains ambiguous
  const inputs = Array.isArray(body.input) ? body.input : [body.input];
  response.writeHead(200, {'content-type':'application/json'});
  response.end(JSON.stringify({data:inputs.map((_,index)=>({index,embedding:[0.25,0.75]}))}));
});
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check, label, timeout = 45000) {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    if (interrupted) throw Error('Process smoke interrupted; cleaning owned resources');
    if (await check()) return;
    await delay(100);
  }
  throw Error(`Timed out: ${label}`);
}
function docker(args, input) {
  return execFileSync('docker', args, {input, encoding:'utf8', stdio:['pipe','pipe','pipe']}).trim();
}
function sql(statement) {
  return docker(['exec','-i',container,'psql','-v','ON_ERROR_STOP=1','-U','memory','-d','memory_test','-At'], statement);
}
async function freePort() {
  const server = net.createServer();
  await new Promise(resolve => server.listen(0,'127.0.0.1',resolve));
  const port = server.address().port;
  await new Promise(resolve => server.close(resolve));
  return port;
}
async function launch(name, databaseUrl, providerPort, enabled = true) {
  const port = await freePort();
  const config = join(directory, `${name}.toml`);
  writeFileSync(config, `[service]
bind_addr = "127.0.0.1:${port}"
api_token = "${token}"
[database]
url = "${databaseUrl}"
[offline]
enabled = false
path = "${join(directory, name+'.duckdb')}"
[cluster]
enabled = false
service_id = "${name}"
[automation]
enabled = false
[mcp]
enabled = false
http_enabled = false
[background_work]
enabled = ${enabled}
poll_seconds = 1
lease_seconds = 4
renewal_seconds = 1
initial_delay_seconds = 1
max_delay_seconds = 2
max_attempts = 5
[embeddings]
create_enabled = true
[[embeddings.backends]]
name = "fixture"
provider = "ollama"
base_url = "http://127.0.0.1:${providerPort}/v1"
model = "fixture"
create_enabled = true
`, {mode:0o600});
  const child = spawn(binary, ['--config',config,'service','run'], {
    cwd:directory, env:{PATH:process.env.PATH, MEMORY_LAYER_PROFILE:'prod'}, stdio:['ignore','pipe','pipe'],
  });
  children.push(child);
  let output = '';
  child.stdout.on('data', chunk => { output += chunk; });
  child.stderr.on('data', chunk => { output += chunk; });
  const base = `http://127.0.0.1:${port}`;
  await until(async () => {
    if (child.exitCode !== null || child.signalCode !== null) throw Error(`${name} exited: ${output}`);
    try { return (await fetch(`${base}/healthz`,{signal:AbortSignal.timeout(1000)})).ok; }
    catch { return false; }
  },`${name} startup`);
  return {child,base};
}
async function stop(service) {
  if (service.child.exitCode !== null || service.child.signalCode !== null) return;
  await new Promise(resolve => { service.child.once('exit',resolve); service.child.kill('SIGKILL'); });
}
async function api(service, path, body) {
  const response = await fetch(service.base+path, {
    method:body ? 'POST':'GET', headers:{'x-api-token':token,'content-type':'application/json'},
    ...(body ? {body:JSON.stringify(body)}:{}), signal:AbortSignal.timeout(5000),
  });
  const result = await response.json();
  assert.equal(response.status,200,JSON.stringify(result));
  return result;
}
async function curate(service, project) {
  await api(service,'/v1/capture/task',{
    project,task_title:'Synthetic process fault',user_prompt:'Verify recoverable jobs.',writer_id:'fixture',
    agent_summary:`Synthetic ${project} recovery input.`,
    structured_candidates:[{canonical_text:`Synthetic ${project} recovery input.`,summary:'Synthetic process input',memory_type:'implementation',importance:3,confidence:0.9}],
  });
  return api(service,'/v1/curate',{project});
}
async function completed(service, project, id) {
  let detail;
  await until(async () => {
    detail = await api(service,`/v1/projects/${project}/jobs/${id}`);
    assert.ok(!['failed','blocked'].includes(detail.job.status),JSON.stringify(detail));
    return detail.job.status === 'succeeded';
  },`${project} completion`);
  return detail;
}

try {
  docker(['run','--detach','--name',container,'--publish','127.0.0.1::5432','--env','POSTGRES_USER=memory','--env','POSTGRES_PASSWORD=disposable-only','--env','POSTGRES_DB=memory_test',image]);
  started = true;
  // The image briefly starts a socket-only initialization server. Wait for
  // TCP readiness so launch cannot race its intentional shutdown/restart.
  await until(() => { try { docker(['exec',container,'pg_isready','-h','127.0.0.1','-U','memory','-d','memory_test']); return true; } catch { return false; } },'PostgreSQL TCP ready');
  const dbPort = docker(['port',container,'5432/tcp']).split(':').at(-1);
  const databaseUrl = `postgresql://memory:disposable-only@127.0.0.1:${dbPort}/memory_test`;
  await new Promise(resolve => provider.listen(0,'127.0.0.1',resolve));
  const providerPort = provider.address().port;

  // Crash before claim: mutation/events/intents already committed, no worker.
  const stopped = await launch('before-claim',databaseUrl,providerPort,false);
  const queued = await curate(stopped,'before-claim');
  assert.equal(calls.length,0);
  await stop(stopped);
  const first = await launch('first-worker',databaseUrl,providerPort);
  const initial = await completed(first,'before-claim',queued.background_work.job_ids[0]);
  assert.equal(initial.job.attempts,1);
  console.log('PASS: committed queue survives process exit before claim');

  // Establish ownership in A, start competitor B, then kill A mid-provider.
  holdNext = true;
  const count = calls.length;
  const interrupted = await curate(first,'during-provider');
  await until(() => calls.length === count+1,'provider accepted first request');
  const competitor = await launch('competing-worker',databaseUrl,providerPort);
  await delay(1500); // longer than a poll, shorter than the renewed lease
  assert.equal(calls.length,count+1,'competing process must not execute the same project');
  await stop(first);
  const recovered = await completed(competitor,'during-provider',interrupted.background_work.job_ids[0]);
  assert.equal(recovered.job.attempts,2);
  assert.equal(calls.length,count+2);
  assert.ok(recovered.history.some(a=>a.failure==='lease_lost' && a.uncertain));
  console.log('PASS: competing process preserves the lease and recovers killed provider owner');
  await stop(competitor);

  // Block final acknowledgement only; the embedding result/checkpoint is
  // already committed when the trigger is reached. Kill that exact process.
  sql(`CREATE FUNCTION r1_delay_ack() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
    IF NEW.kind='embedding' AND NEW.status='succeeded' AND OLD.status='running' THEN PERFORM pg_sleep(8); END IF;
    RETURN NEW; END $$;
    CREATE TRIGGER r1_delay_ack BEFORE UPDATE ON background_jobs FOR EACH ROW EXECUTE FUNCTION r1_delay_ack();`);
  const finisher = await launch('before-ack',databaseUrl,providerPort);
  const prior = calls.length;
  const afterResult = await curate(finisher,'after-result');
  const version = afterResult.memory_ids[0];
  const job = afterResult.background_work.job_ids[0];
  // Curation itself creates unembedded text chunks. Wait for the worker's
  // applied marker, committed atomically with vectors, not mere chunk existence.
  await until(() => sql(`SELECT count(*) FROM background_job_outputs WHERE job_id='${job}' AND output_key='applied:${version}' AND output='true'::jsonb;`)==='1','embedding result committed');
  await stop(finisher);
  const replacement = await launch('after-ack-crash',databaseUrl,providerPort);
  const final = await completed(replacement,'after-result',afterResult.background_work.job_ids[0]);
  assert.equal(final.job.attempts,2);
  assert.equal(calls.length,prior+1,'completed version must not be embedded/billed again');
  assert.equal(sql(`SELECT count(*) FROM memory_chunks WHERE memory_entry_id='${version}';`),'1');
  assert.equal(sql(`SELECT count(*) FROM memory_chunk_embeddings e JOIN memory_chunks c ON c.id=e.chunk_id WHERE c.memory_entry_id='${version}';`),'1');
  console.log('PASS: result survives crash before acknowledgement without duplicate provider work');
  console.log(`PASS: R1 real-process smoke; synthetic configs retained at ${directory}`);
} finally {
  for (const child of children) await stop({child});
  provider.closeAllConnections();
  await new Promise(resolve => provider.close(resolve));
  if (started) docker(['rm','--force','--volumes',container]);
}
