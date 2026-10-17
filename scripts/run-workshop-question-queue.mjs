// Trusted operator only. This is not an app/MCP tool or a source of questions.
// Each run-next invocation admits at most one explicitly confirmed private
// question, under the pinned runtime's source, credential, lease and USD guards.
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const root = '/home/olivier/.local/state/memory-workshop-preview-20260918';
const stage = `${root}/questions-60`;
const hash = bytes => createHash('sha256').update(bytes).digest('hex');

function readPrivate(path, executable = false) {
  const fd = fs.openSync(path, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
  try {
    const stat = fs.fstatSync(fd);
    if (!stat.isFile() || stat.uid !== process.getuid() || (stat.mode & 0o077) !== 0
      || stat.size > (executable ? 300_000_000 : 2_000_000)) throw Error('Private operator artifact rejected');
    return fs.readFileSync(fd);
  } finally { fs.closeSync(fd); }
}

export function credential(bytes) {
  const lines = bytes.toString('utf8').split(/\r?\n/).filter(line => /^(?:export\s+)?OPENAI_API_KEY\s*=/.test(line));
  if (lines.length !== 1) throw Error('Exactly one approved normal credential is required; no fallback');
  let value = lines[0].replace(/^(?:export\s+)?OPENAI_API_KEY\s*=\s*/, '').trim();
  if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) value = value.slice(1, -1);
  if (!/^sk-[A-Za-z0-9_-]{20,}$/.test(value)) throw Error('Normal credential unavailable; no fallback');
  return value;
}

export function metadata(bytes) {
  if (bytes.length > 16_384) throw Error('Worker metadata exceeded bounds');
  const report = JSON.parse(bytes.toString('utf8'));
  const allowed = new Set(['idle', 'busy', 'blocked', 'credential_unavailable', 'storage_unavailable', 'stopped', 'processed']);
  if (!allowed.has(report.outcome)) throw Error('Unrecognized worker result');
  // Never relay arbitrary child stdout, stderr, private bodies or added fields.
  const clean = { outcome: report.outcome };
  if (report.work) {
    for (const key of ['action_id', 'attempt_id']) {
      if (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(report.work[key])) throw Error('Invalid worker identity');
      clean[key] = report.work[key];
    }
    if (!/^[a-z_]{1,40}$/.test(report.work.outcome)) throw Error('Invalid worker outcome');
    clean.work_outcome = report.work.outcome;
  }
  return clean;
}

async function main() {
  const mode = process.argv[2];
  if (!['once', 'serve'].includes(mode) || process.argv.length !== 3 || process.getuid() === 0) throw Error('Choose once or serve as the non-root operator');
  let stopped = false;
  let child;
  for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => { stopped = true; child?.kill(signal); });
  do {
    const d = JSON.parse(readPrivate(`${root}/deployment.json`));
    const artifacts = JSON.parse(readPrivate(`${stage}/artifact.json`));
    if (d.database !== 'memory_workshop_preview_20260918' || d.instance_id !== '72d1a39c-29fe-4f8d-94ee-b3a8dba51f85') throw Error('Wrong Workshop binding');
    if (hash(readPrivate(`${stage}/worker`, true)) !== artifacts.worker_sha256
      || hash(readPrivate(`${stage}/work.json`)) !== artifacts.manifest_sha256
      || hash(readPrivate(`${root}/app.toml`)) !== artifacts.service_config_sha256) throw Error('Pinned operator configuration changed; inspect before restarting');
    const env = { PATH: '/usr/bin:/bin', HOME: '/home/olivier',
      DATABASE_URL: `postgresql://workshop_worker:${encodeURIComponent(d.worker_password)}@127.0.0.1:55460/${d.database}`,
      OPENAI_API_KEY: credential(readPrivate('/home/olivier/.config/codifix/codifix.env')) };
    if (stopped) break;
    child = spawn(`${stage}/worker`, ['run-next', `${stage}/work.json`, artifacts.manifest_sha256,
      d.instance_id, `${root}/app.toml`, artifacts.service_config_sha256],
    { cwd: root, env, stdio: ['ignore', 'pipe', 'pipe'] });
    const chunks = []; let length = 0;
    child.stdout.on('data', chunk => { length += chunk.length; if (length <= 16_384) chunks.push(chunk); else child.kill('SIGTERM'); });
    child.stderr.resume(); // diagnostics stay private, never journal provider bodies
    const code = await new Promise((resolve, reject) => child.once('error', reject).once('close', resolve));
    child = undefined;
    if (length > 16_384) throw Error('Worker output rejected');
    const report = metadata(Buffer.concat(chunks));
    if (!['idle', 'busy'].includes(report.outcome) || mode === 'once') console.log(JSON.stringify(report));
    // Storage/authority/credential failure needs operator attention, not an
    // unbounded error loop. Successfully classified individual denied work can
    // yield Processed and leave unrelated questions eligible.
    if (code !== 0 || ['blocked', 'credential_unavailable', 'storage_unavailable'].includes(report.outcome)) throw Error('Worker stopped; inspect private status before restarting');
    if (mode === 'once' || stopped) break;
    await new Promise(resolve => setTimeout(resolve, 2000));
  } while (!stopped);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch(() => { console.error('Workshop question worker stopped; private diagnostics suppressed. Inspect supervisor status.'); process.exitCode = 1; });
}
