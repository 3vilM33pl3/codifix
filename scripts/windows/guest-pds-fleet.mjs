// SPDX-License-Identifier: AGPL-3.0-or-later
// Disposable real-PDS provisioning and existing native Memory fleet tests.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import http from 'node:http';
import { once } from 'node:events';
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { pathToFileURL } from 'node:url';
await (async () => {
const [source, root, sid, mode, profilePath] = process.argv.slice(2);
const accountOnly = mode === '--account-only';
if (process.platform !== 'win32' || process.env.COMPUTERNAME !== 'MEMORY-WINLAB'
    || !/^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+$/.test(source ?? '')
    || !/^C:\\MemoryLab\\runs\\[0-9a-f-]{36}\\native-pds-fleet$/.test(root ?? '')
    || !/^S-1-5-21-(?:\d+-){3}\d+$/.test(sid ?? '')
    || (mode !== undefined && mode !== '--fleet' && !accountOnly)
    || (!accountOnly && process.env.MEMORY_LAYER_TEST_REQUIRE_DB !== '1')) throw Error('Explicit disposable native fleet required');
const node = 'C:\\MemoryLab\\Tools\\node-v24.18.1-win-x64\\node.exe';
const binary = 'C:\\MemoryLab\\build\\product-msvc\\x86_64-pc-windows-msvc\\debug\\codifix-pds-pipe.exe';
const digest = async file => createHash('sha256').update(await fs.readFile(file)).digest('hex');
const json = async file => JSON.parse((await fs.readFile(file, 'utf8')).replace(/^\uFEFF/, ''));
const baselinePath = 'C:\\MemoryLab\\NativePds\\prepared.json';
let preparedPath = baselinePath;
if (profilePath !== undefined) {
  assert.equal(profilePath, 'C:\\MemoryLab\\inputs\\native-pds-profile.json');
  const selected = await json(profilePath);
  assert.deepEqual(Object.keys(selected).sort(), ['format','prepared','sha256']);
  assert.equal(selected.format, 'memory-native-pds-profile-v1');
  assert(/^C:\\MemoryLab\\NativePds\\(?:patched-[0-9a-f-]{36}\\)?prepared\.json$/.test(selected.prepared));
  assert.equal(await digest(selected.prepared), selected.sha256);
  preparedPath = selected.prepared;
}
const runtime = path.join(path.dirname(preparedPath), 'runtime');
const prepared = await json(preparedPath);
if (preparedPath === baselinePath) assert.equal(prepared.format, 'memory-windows-pds-proof-result-v1');
else {
  assert.equal(prepared.format, 'memory-windows-pds-patched-proof-v1');
  assert.equal(prepared.original_prepared_sha256, await digest(baselinePath));
  assert.equal(prepared.path_patch.layout, 'windows-sha256-did-paths-v1');
  const patchFile = path.join(source, 'packaging/windows-pds/windows-paths.mjs');
  assert.equal(prepared.path_patch.patch_sha256, await digest(patchFile));
  const { patchPdsFile, originalFiles } = await import(pathToFileURL(patchFile));
  const expected = new Map((await json(baselinePath)).files.map(f => [f.path, f.sha256]));
  for (const name of Object.keys(originalFiles)) {
    const file = `node_modules/@atproto/pds/dist/${name}`;
    const bytes = patchPdsFile(name, await fs.readFile(path.join(path.dirname(baselinePath), 'runtime', file)));
    expected.set(file, createHash('sha256').update(bytes).digest('hex'));
  }
  assert.equal(prepared.files.length, expected.size);
  for (const file of prepared.files) {
    assert.equal(file.sha256, expected.get(file.path), 'unreviewed PDS portability delta');
    expected.delete(file.path);
  }
  assert.equal(expected.size, 0);
}
assert.equal(prepared.native_sqlite_verified, true);
assert.equal(await digest(node), prepared.node_sha256);
const actualFiles = new Set();
const inventoryTree = async directory => {
  const metadata = await fs.lstat(directory);
  assert(metadata.isDirectory() && !metadata.isSymbolicLink(), 'runtime reparse directory');
  const entries = await fs.readdir(directory, { withFileTypes: true });
  for (let offset = 0; offset < entries.length; offset += 16) {
    const selected = await Promise.all(entries.slice(offset, offset + 16).map(async entry => {
      const file = path.join(directory, entry.name);
      return { file, info: await fs.lstat(file) };
    }));
    for (const { file, info } of selected) {
      assert(!info.isSymbolicLink(), 'runtime reparse entry');
      if (info.isDirectory()) await inventoryTree(file);
      else {
        assert(info.isFile(), 'runtime special entry');
        assert(actualFiles.size < 65536, 'runtime inventory limit');
        actualFiles.add(path.relative(runtime, file).replaceAll('\\', '/'));
      }
    }
  }
};
await inventoryTree(runtime);
assert.equal(actualFiles.size, prepared.files.length, 'runtime file inventory changed');
for (let offset = 0; offset < prepared.files.length; offset += 16) {
  await Promise.all(prepared.files.slice(offset, offset + 16).map(async item => {
    assert(!path.win32.isAbsolute(item.path) && !item.path.includes('..') && !item.path.includes(':'));
    assert(actualFiles.delete(item.path), 'missing or duplicate prepared runtime file');
    assert.equal(await digest(path.join(runtime, item.path)), item.sha256);
  }));
}
assert.equal(actualFiles.size, 0);
const build = JSON.parse((await fs.readFile('C:\\MemoryLab\\runs\\79a6444a-a60b-4494-a8e3-de90d1b4c807\\product-build\\result.json', 'utf8')).replace(/^\uFEFF/, ''));
assert.equal(build.state, 'succeeded');
assert.equal(await digest(binary), build.artifacts.find(a => a.file === 'codifix-pds-pipe.exe').sha256);
const require = createRequire(path.join(runtime, 'pds-server.mjs'));
const pdsRequire = createRequire(require.resolve('@atproto/pds'));
const { Secp256k1Keypair } = pdsRequire('@atproto/crypto');
const { createServiceJwt } = pdsRequire('@atproto/xrpc-server');
let document;
const didServer = http.createServer((req, res) => {
  if (req.url !== '/.well-known/did.json') { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { 'content-type': 'application/did+ld+json' });
  res.end(JSON.stringify(document));
});
await new Promise(resolve => didServer.listen(0, 'localhost', resolve));
const did = `did:web:localhost%3A${didServer.address().port}`;
const handle = 'space.federation.example.com';
const origin = 'https://pds.federation.example.com';
const setDocument = key => { document = { '@context': ['https://www.w3.org/ns/did/v1'], id: did,
  alsoKnownAs: [`at://${handle}`], verificationMethod: [{ id: `${did}#atproto`, type: 'Multikey',
    controller: did, publicKeyMultibase: key.slice('did:key:'.length) }],
  service: [{ id: `${did}#atproto_pds`, type: 'AtprotoPersonalDataServer', serviceEndpoint: origin }] }; };
const admin = randomBytes(32).toString('hex');
const environment = { SystemRoot: process.env.SystemRoot, PATH: path.dirname(node),
  TEMP: path.join(root, 'temp'), TMP: path.join(root, 'temp'), LOG_LEVEL: 'warn', LOG_ENABLED: 'true',
  PDS_HOSTNAME: 'pds.federation.example.com', PDS_SERVICE_HANDLE_DOMAINS: '.federation.example.com',
  PDS_DATA_DIRECTORY: path.join(root, 'data'), PDS_BLOBSTORE_DISK_LOCATION: path.join(root, 'data/blobs'),
  PDS_JWT_SECRET: randomBytes(32).toString('hex'), PDS_ADMIN_PASSWORD: admin,
  PDS_PLC_ROTATION_KEY_K256_PRIVATE_KEY_HEX: randomBytes(32).toString('hex'),
  PDS_INVITE_REQUIRED: 'true', PDS_DEV_MODE: 'false', PDS_DISABLE_SSRF_PROTECTION: 'false',
  PDS_CRAWLERS: '', PDS_DID_PLC_URL: 'https://unavailable.federation.example.com' };
await fs.mkdir(environment.TEMP);
await fs.mkdir(environment.PDS_DATA_DIRECTORY);
let child, broker;
const children = new Set();
function tracked(exe, args, options) {
  const p = spawn(exe, args, { windowsHide: true, ...options });
  children.add(p); p.once('exit', () => children.delete(p)); return p;
}
const killTree = async p => {
  const killer = spawn(path.join(process.env.SystemRoot, 'System32/taskkill.exe'), ['/PID', String(p.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' });
  await once(killer, 'exit');
};
let expired = false, deadlineCleanup;
const deadline = setTimeout(() => {
  expired = true;
  // Kill the owned trees while their roots are still tracked. Killing only the
  // root first loses the ancestry needed to reap cargo/PDS descendants.
  deadlineCleanup = Promise.all([...children].map(killTree));
}, 30 * 60000);
try {
  const initialKey = await Secp256k1Keypair.create({ exportable: true });
  setDocument(initialKey.did());
  child = tracked(node, [path.join(runtime, 'pds-server.mjs')], { cwd: runtime, env: environment, stdio: ['pipe','pipe','pipe'] });
  let pending = Buffer.alloc(0), wake;
  const frames = [];
  child.stdout.on('data', bytes => {
    pending = Buffer.concat([pending, bytes]);
    while (pending.length >= 4) {
      const size = pending.readUInt32BE();
      if (size > 16 * 1024 * 1024) throw Error('PDS frame limit');
      if (pending.length < size + 4) break;
      frames.push(pending.subarray(4, size + 4)); pending = pending.subarray(size + 4); wake?.();
    }
  });
  const diagnostics = await fs.open(path.join(root, 'provision.stderr.log'), 'wx');
  let diagnosticBytes = 0, diagnosticWrite = Promise.resolve();
  child.stderr.on('data', bytes => {
    diagnosticBytes += bytes.length;
    if (diagnosticBytes <= 1024 * 1024) diagnosticWrite = diagnosticWrite.then(() => diagnostics.write(bytes));
  });
  child.stderr.once('end', () => { void diagnosticWrite.finally(() => diagnostics.close()); });
  const frame = async () => {
    const until = Date.now() + 120000;
    while (!frames.length) {
      if (child.exitCode !== null || Date.now() > until) throw Error('Native PDS frame unavailable');
      await new Promise(resolve => { const timer = setTimeout(resolve, 100); wake = () => { clearTimeout(timer); resolve(); }; });
    }
    return frames.shift();
  };
  assert.equal((await frame()).toString(), 'memory-pds-stdio-v1');
  const rpc = async (nsid, { body, token, isAdmin = false, method } = {}) => {
    const payload = body === undefined ? Buffer.alloc(0) : Buffer.from(JSON.stringify(body));
    const authorization = isAdmin ? `Basic ${Buffer.from(`admin:${admin}`).toString('base64')}` : token ? `Bearer ${token}` : undefined;
    const request = Buffer.concat([Buffer.from(`${method ?? (body === undefined ? 'GET' : 'POST')} /xrpc/${nsid} HTTP/1.1\r\nHost: pds.federation.example.com\r\nConnection: close\r\n${body === undefined ? '' : 'Content-Type: application/json\r\n'}Content-Length: ${payload.length}\r\n${authorization ? `Authorization: ${authorization}\r\n` : ''}\r\n`), payload]);
    const length = Buffer.alloc(4); length.writeUInt32BE(request.length); child.stdin.write(Buffer.concat([length, request]));
    const response = await frame(), separator = response.indexOf('\r\n\r\n');
    assert(separator > 0);
    const status = Number(response.subarray(0, response.indexOf('\r\n')).toString().split(' ')[1]);
    const result = JSON.parse(response.subarray(separator + 4));
    if (status < 200 || status >= 300) throw Error(`${nsid}: HTTP ${status} (${String(result.error).slice(0, 100)})`);
    return result;
  };
  const invite = await rpc('com.atproto.server.createInviteCode', { isAdmin: true, body: { useCount: 1 } });
  const jwt = await createServiceJwt({ iss: did, aud: 'did:web:pds.federation.example.com', lxm: 'com.atproto.server.createAccount', keypair: initialKey });
  const account = await rpc('com.atproto.server.createAccount', { token: jwt, body: {
    did, handle, email: 'native@federation.example.com', password: randomBytes(24).toString('hex'), inviteCode: invite.code } });
  assert.equal(account.did, did);
  const credentials = await rpc('com.atproto.identity.getRecommendedDidCredentials', { token: account.accessJwt });
  const signingKey = credentials.verificationMethods.atproto;
  assert.notEqual(signingKey, initialKey.did());
  setDocument(signingKey);
  // This procedure has no input schema: POST without a JSON body, not {}.
  await rpc('com.atproto.server.activateAccount', { token: account.accessJwt, method: 'POST' });
  const exited = once(child, 'exit'); child.stdin.end();
  assert.equal((await exited)[0], 0);
  if (accountOnly) {
    await fs.writeFile(path.join(root, 'account-proof.json'), JSON.stringify({
      format: 'memory-native-pds-account-proof-v1', state: 'succeeded', repository: did,
      signing_key: signingKey, key_handoff_verified: true, ssrf_disabled: false,
      fleet_verified: false, native_pipe_verified: false,
    }), { flag: 'wx' });
    return;
  }
  const pipe = { name: `\\\\.\\pipe\\memory-pds-${path.basename(path.dirname(root))}`, server_sid: sid, client_sid: sid };
  const config = { schema_version: 1, pipe, node, node_sha256: await digest(node),
    entry: path.join(runtime, 'pds-server.mjs'), entry_sha256: await digest(path.join(runtime, 'pds-server.mjs')),
    helper_sha256: await digest(path.join(runtime, 'http-stdio.mjs')) };
  const configPath = path.join(root, 'pipe.json');
  await fs.writeFile(configPath, JSON.stringify(config), { flag: 'wx' });
  await fs.writeFile(path.join(root, 'fixture.json'), JSON.stringify({ base: origin, repository: did,
    token: account.accessJwt, signing_key: signingKey, pds_named_pipe: pipe }), { flag: 'wx' });
  broker = tracked(binary, [configPath], { cwd: root, env: environment, stdio: ['ignore','ignore','pipe'] });
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(Error('Native PDS broker readiness timeout')), 120000);
    let diagnostics = '';
    broker.stderr.on('data', bytes => { diagnostics += bytes.toString(); if (Buffer.byteLength(diagnostics) > 65536) { clearTimeout(timer); reject(Error('Broker diagnostic limit')); return; }
      if (diagnostics.includes('native private PDS pipe ready')) { clearTimeout(timer); resolve(); } });
    broker.once('exit', () => { clearTimeout(timer); reject(Error('Native PDS broker exited before readiness')); });
  });
  const results = [];
  for (const filter of ['e_two_instance_http_workflow','f1_independent_instances','watched_agents_four_area_signed_pds_rehearsal']) {
    const log = await fs.open(path.join(root, `${filter}.log`), 'wx');
    try {
      const cargo = tracked('C:\\Users\\memorylab\\.cargo\\bin\\cargo.exe', ['test','--locked','-p','codifix-service','--lib',filter,'--','--test-threads=1','--nocapture'], {
        cwd: source, env: { ...process.env, MEMORY_FEDERATION_PDS_FIXTURE: path.join(root, 'fixture.json'),
          MEMORY_FEDERATION_F1_INSTANCES: '1', MEMORY_FEDERATION_AGENT_INSTANCES: '1',
          CARGO_TARGET_DIR: 'C:\\MemoryLab\\build\\product-msvc', CARGO_BUILD_TARGET: 'x86_64-pc-windows-msvc', CARGO_BUILD_JOBS: '2' }, stdio: ['ignore',log.fd,log.fd] });
      assert.equal((await once(cargo, 'exit'))[0], 0, `${filter} failed; inspect private log`);
    } finally { await log.close(); }
    const text = await fs.readFile(path.join(root, `${filter}.log`), 'utf8');
    assert.match(text, /test result: ok\. [1-9][0-9]* passed;/);
    results.push({ filter, passed: true });
  }
  assert.equal(expired, false, 'fleet deadline invalidates operation');
  await fs.writeFile(path.join(root, 'fleet-proof.json'), JSON.stringify({ format: 'memory-native-pds-fleet-proof-v1', state: 'succeeded',
    results, native_pds_pipe: true, byod_provisioned: true, plc_used: false, ssrf_disabled: false,
    real_provider_requests: 0, c_workflow_verified: false, cross_os_verified: false,
    installed_services_verified: false, signing_key: signingKey, repository: did, broker_sha256: await digest(binary) }), { flag: 'wx' });
} finally {
  clearTimeout(deadline);
  await deadlineCleanup;
  for (const p of children) {
    // Each child is owned by this disposable operation. taskkill's /T is
    // necessary for cargo/rustc and for the broker's credential-free Node child.
    await killTree(p);
  }
  await new Promise(resolve => didServer.close(resolve));
}
})();
