// Explicit operator phases for the existing independent Workshop preview only.
// This never starts a paid worker, copies a provider key, creates a timer, or
// touches another area. Preparation of this script does not execute deployment.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';

const root = '/home/olivier/.local/state/memory-workshop-preview-20260918';
const repository = '/home/olivier/Projects/memory';
const built = '/data/tmp/memory-four-area-target/debug';
const app = 'memory-workshop-preview-app-20260918';
const retained = 'memory-workshop-preview-app-before-questions-60';
const backup = `${root}/before-questions-60`;
const staging = `${root}/questions-60`;
const offsite = '/media/bunnychow/backups/memory-four-area-20260917/workshop-preview-20260918/before-questions-60.tar.gpg';
const image = 'ubuntu@sha256:cd21a4f68a617580279d4b091cb18e3af9fa8a87500665f0ae5f7f757d17d367';
const phase = process.argv[2];
if (!['preserve', 'upgrade', 'start'].includes(phase) || process.argv.length !== 3) {
  throw Error('Choose preserve, upgrade or start explicitly');
}
const deployment = JSON.parse(fs.readFileSync(`${root}/deployment.json`));
if (deployment.database !== 'memory_workshop_preview_20260918'
  || deployment.instance_id !== '72d1a39c-29fe-4f8d-94ee-b3a8dba51f85'
  || deployment.container !== 'memory-workshop-preview-db-20260918'
  || deployment.network !== 'memory-workshop-preview-20260918') {
  throw Error('Wrong preview identity');
}
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
function run(program, args, options = {}) {
  const result = spawnSync(program, args, { maxBuffer: 128 * 1024 * 1024, ...options });
  if (result.status !== 0) {
    throw Error(`${path.basename(program)} failed; private diagnostics suppressed. Inspect before retrying.`);
  }
  return result.stdout;
}
function inspectApp() {
  const record = JSON.parse(run('/usr/bin/docker', ['inspect', app], { encoding: 'utf8' }))[0];
  if (!record.Mounts.some(mount => mount.Source === `${root}/app.toml` && mount.Destination === '/config.toml')) {
    throw Error('Named container is not the enrolled Workshop preview');
  }
  return record;
}
function backupVerified() {
  const receipt = JSON.parse(fs.readFileSync(`${backup}/receipt.json`));
  if (!receipt.copy_verified || digest(fs.readFileSync(`${backup}.tar.gpg`)) !== receipt.sha256
    || digest(fs.readFileSync(offsite)) !== receipt.sha256) {
    throw Error('Encrypted local and NAS backups are not verified');
  }
}
function treeDigest(directory) {
  const hash = createHash('sha256');
  function visit(current, relative = '') {
    for (const entry of fs.readdirSync(current, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const name = relative ? `${relative}/${entry.name}` : entry.name;
      if (entry.isDirectory()) visit(path.join(current, entry.name), name);
      else if (entry.isFile()) {
        const bytes = fs.readFileSync(path.join(current, entry.name));
        hash.update(`${name.length}:${name}:${bytes.length}:`);
        hash.update(bytes);
      } else throw Error('Unexpected non-regular Web artifact');
    }
  }
  visit(directory);
  return hash.digest('hex');
}
function configMatches(hashes) {
  const config = fs.readFileSync(`${root}/app.toml`);
  if (digest(config) !== hashes.service_config_sha256
    || !config.toString().includes('[supervisor.interaction]')
    || !config.toString().includes('/interaction/work.json')
    || !config.toString().includes(hashes.manifest_sha256)) {
    throw Error('Pinned live service configuration changed or does not bind the staged profile');
  }
}

if (phase === 'preserve') {
  if (fs.existsSync(backup) || fs.existsSync(`${backup}.tar.gpg`) || fs.existsSync(offsite)) {
    throw Error('Backup already exists; inspect, do not overwrite');
  }
  inspectApp();
  const mounts = run('/usr/bin/findmnt', ['-n', '-o', 'TARGET', '-T', path.dirname(offsite)], { encoding: 'utf8' })
    .trim().split('\n').filter(Boolean);
  if (mounts.length === 0 || mounts.some(mount => mount !== '/media/bunnychow')) {
    throw Error('Expected NAS mount is not available');
  }
  fs.mkdirSync(backup, { mode: 0o700 });
  run('/usr/bin/docker', ['stop', app]);
  try {
    const dump = run('/usr/bin/docker', ['exec', deployment.container, 'pg_dump', '-U', 'postgres', '-d', deployment.database, '-Fc']);
    fs.writeFileSync(`${backup}/database.dump`, dump, { mode: 0o600, flag: 'wx' });
    for (const name of ['app.toml', 'client.toml', 'deployment.json', 'bootstrap.json', 'work.json', 'artifact-hashes.json']) {
      fs.copyFileSync(`${root}/${name}`, `${backup}/${name}`, fs.constants.COPYFILE_EXCL);
      fs.chmodSync(`${backup}/${name}`, 0o600);
    }
    fs.copyFileSync(`${root}/interaction-59/artifact.json`, `${backup}/interaction-59-artifact.json`, fs.constants.COPYFILE_EXCL);
    const tar = run('/usr/bin/tar', ['-C', backup, '-cf', '-', '.']);
    const encrypted = run('/usr/bin/gpg', ['--batch', '--trust-model', 'always', '--encrypt', '--recipient', '97D83F9D129AB85B38907FF77AEF04712CEA51D2'], { input: tar });
    fs.writeFileSync(`${backup}.tar.gpg`, encrypted, { mode: 0o600, flag: 'wx' });
    fs.copyFileSync(`${backup}.tar.gpg`, offsite, fs.constants.COPYFILE_EXCL);
    if (digest(fs.readFileSync(offsite)) !== digest(encrypted)) throw Error('NAS encrypted copy mismatch');
    const receipt = { sha256: digest(encrypted), bytes: encrypted.length, copy_verified: true, restore_rehearsed: false, includes_provider_key: false };
    fs.writeFileSync(`${backup}/receipt.json`, JSON.stringify(receipt), { mode: 0o600, flag: 'wx' });
    console.log(JSON.stringify(receipt));
  } catch (error) {
    run('/usr/bin/docker', ['start', app]);
    throw error;
  }
}

if (phase === 'upgrade') {
  backupVerified();
  if (inspectApp().State.Running) throw Error('Stop the exact preview before migrating');
  if (fs.existsSync(staging)) throw Error('Staging already exists; inspect, do not overwrite');
  for (const file of [`${built}/memory`, `${built}/memory-workshop-worker`, `${built}/examples/upgrade_workshop_questions`, `${repository}/web/dist/index.html`]) {
    if (!fs.statSync(file).isFile()) throw Error('Required build artifact unavailable');
  }
  const output = run(`${built}/examples/upgrade_workshop_questions`, ['--writers-stopped-backup-verified'], {
    env: { PATH: '/usr/bin:/bin', DATABASE_URL: `postgresql://postgres:${encodeURIComponent(deployment.admin_password)}@127.0.0.1:55460/${deployment.database}` },
  });
  const migration = JSON.parse(output);
  if (migration.migration_head !== 60 || migration.instance_id !== deployment.instance_id) throw Error('Unexpected migration result');
  fs.writeFileSync(`${backup}/upgrade.json`, output, { mode: 0o600, flag: 'wx' });
  fs.mkdirSync(staging, { mode: 0o700 });
  fs.copyFileSync(`${root}/work.json`, `${staging}/work.json`, fs.constants.COPYFILE_EXCL);
  for (const [source, target] of [[`${built}/memory`, 'memory'], [`${built}/memory-workshop-worker`, 'worker']]) {
    fs.copyFileSync(source, `${staging}/${target}`, fs.constants.COPYFILE_EXCL);
    fs.chmodSync(`${staging}/${target}`, 0o500);
  }
  fs.cpSync(`${repository}/web/dist`, `${staging}/web`, { recursive: true, errorOnExist: true, force: false });
  const hashes = {
    memory_sha256: digest(fs.readFileSync(`${staging}/memory`)),
    worker_sha256: digest(fs.readFileSync(`${staging}/worker`)),
    manifest_sha256: digest(fs.readFileSync(`${staging}/work.json`)),
    service_config_sha256: digest(fs.readFileSync(`${root}/app.toml`)),
    web_sha256: treeDigest(`${staging}/web`),
  };
  configMatches(hashes);
  fs.writeFileSync(`${staging}/artifact.json`, JSON.stringify(hashes), { mode: 0o600, flag: 'wx' });
  console.log(JSON.stringify(migration));
}

if (phase === 'start') {
  backupVerified();
  const upgrade = JSON.parse(fs.readFileSync(`${backup}/upgrade.json`));
  if (upgrade.migration_head !== 60 || upgrade.instance_id !== deployment.instance_id) throw Error('Migration receipt unavailable');
  if (inspectApp().State.Running) throw Error('Expected the old preview to remain stopped');
  const hashes = JSON.parse(fs.readFileSync(`${staging}/artifact.json`));
  if (digest(fs.readFileSync(`${staging}/memory`)) !== hashes.memory_sha256
    || digest(fs.readFileSync(`${staging}/worker`)) !== hashes.worker_sha256
    || digest(fs.readFileSync(`${staging}/work.json`)) !== hashes.manifest_sha256
    || treeDigest(`${staging}/web`) !== hashes.web_sha256) {
    throw Error('Staged artifacts changed');
  }
  configMatches(hashes);
  run('/usr/bin/docker', ['update', '--restart=no', app]);
  run('/usr/bin/docker', ['rename', app, retained]);
  const args = ['run', '-d', '--pull=never', '--name', app, '--restart', 'unless-stopped', '--network', deployment.network,
    '--user', '1000:1000', '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges', '--read-only', '--pids-limit', '128',
    '--memory', '1g', '--tmpfs', '/tmp:rw,noexec,nosuid,size=64m', '--workdir', '/state'];
  for (const [source, target] of [[`${staging}/memory`, '/memory'], [`${root}/artifacts/loopback-stdio`, '/loopback-stdio'],
    [`${root}/app.toml`, '/config.toml'], [`${staging}/web`, '/web'], [staging, '/interaction']]) {
    args.push('--mount', `type=bind,src=${source},dst=${target},readonly`);
  }
  args.push('--mount', `type=bind,src=${root}/state,dst=/state`);
  for (const value of ['HOME=/state', 'XDG_CONFIG_HOME=/state/config', 'XDG_DATA_HOME=/state/data', 'XDG_STATE_HOME=/state/state', 'MEMORY_LAYER_PROFILE=prod']) {
    args.push('--env', value);
  }
  args.push(image, '/memory', '--config', '/config.toml', 'service', 'run');
  run('/usr/bin/docker', args);
  run('/usr/bin/systemctl', ['--user', 'restart', 'memory-workshop-preview-relay.service']);
  console.log(JSON.stringify({ started: app, old_container_retained: retained, provider_calls: 0, provider_key_in_app: false, paid_worker_started: false }));
}
