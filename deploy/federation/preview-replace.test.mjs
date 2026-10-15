// SPDX-License-Identifier: AGPL-3.0-or-later
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { artifactHash, execute, validate, validateBoundary } from './preview-replace.mjs';

const hash = value => createHash('sha256').update(value).digest('hex');
const oldId = 'a'.repeat(64), newId = 'b'.repeat(64), imageId = `sha256:${'c'.repeat(64)}`;
function fixture(t, kind = 'memory-company-20260919-gateway-1') {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'replace-preview-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const artifact = path.join(directory, 'new-config'), receipt = path.join(directory, 'backup-receipt.json');
  fs.writeFileSync(artifact, 'synthetic new configuration', { mode: 0o600 });
  fs.writeFileSync(receipt, '{"synthetic_backup":true}', { mode: 0o600 });
  const contract = { schema_version: 1, name: kind, old_id: oldId, retained_name: `${kind}-before-dependable`, directory,
    backup_receipt: receipt, backup_receipt_sha256: hash(fs.readFileSync(receipt)), mounts: [{ source: artifact, destination: '/config/current', writable: false, sha256: artifactHash(artifact) }] };
  const networks = ['first', 'second'].map((name, i) => ({ Id: String(i + 1).repeat(64), Name: name, Internal: true, EnableIPv6: false }));
  const before = { Id: oldId, Image: imageId, Name: `/${kind}`, State: { Running: true },
    Config: { User: '1000:1000', Image: 'mutable:tag', Env: ['PATH=/bin', 'SYNTHETIC_SECRET=not-in-argv'], Cmd: ['serve'], Entrypoint: ['/binary'], WorkingDir: '/work', Labels: {} },
    HostConfig: { ReadonlyRootfs: true, Privileged: false, CapDrop: ['ALL'], SecurityOpt: ['no-new-privileges:true'], PortBindings: {},
      Memory: 536870912, PidsLimit: 128, RestartPolicy: { Name: 'unless-stopped', MaximumRetryCount: 0 }, Tmpfs: {} },
    Mounts: [{ Type: 'bind', Source: path.join(directory, 'old-config'), Destination: '/config/current', RW: false }],
    NetworkSettings: { Ports: {}, Networks: Object.fromEntries(networks.map((n, i) => [n.Name, { NetworkID: n.Id, IPAddress: `172.22.${i}.5`, Aliases: [`alias-${i}`] }])) } };
  const containers = new Map([[oldId, before]]), commands = [];
  let failure;
  const docker = {
    containers, commands, failOnce(predicate, after = false) { failure = { predicate, after }; },
    inspect(id) { const c = containers.get(id); if (!c) throw Error('synthetic_not_found'); return structuredClone(c); },
    run(args) {
      commands.push(args);
      const fails = failure && failure.predicate(args); const after = failure?.after;
      if (fails) { failure = null; if (!after) throw Error('synthetic_unconfirmed'); }
      let result = '';
      if (args[0] === 'network' && args[1] === 'inspect') result = JSON.stringify([networks.find(n => n.Id === args[2] || n.Name === args[2])]);
      else if (args[0] === 'image') result = JSON.stringify([{ Id: imageId }]);
      else if (args[0] === 'ps') {
        const wanted = args.at(-1).slice('name=^/'.length, -1);
        result = [...containers.values()].find(c => c.Name === `/${wanted}`)?.Id ?? '';
      } else if (args[0] === 'stop') containers.get(args.at(-1)).State.Running = false;
      else if (args[0] === 'start') {
        const c = containers.get(args[1]); c.State.Running = true;
        for (const [name, net] of Object.entries(c.NetworkSettings.Networks)) {
          net.NetworkID = networks.find(n => n.Name === name).Id;
          net.IPAddress = net.IPAMConfig?.IPv4Address ?? net.IPAddress;
        }
      }
      else if (args[0] === 'update') {
        const [name, count = '0'] = args[1].slice('--restart='.length).split(':');
        containers.get(args[2]).HostConfig.RestartPolicy = { Name: name, MaximumRetryCount: Number(count) };
      } else if (args[0] === 'rename') containers.get(args[1]).Name = `/${args[2]}`;
      else if (args[0] === 'network' && args[1] === 'disconnect') {
        const c = containers.get(args[3]); const name = args[2] === 'none' ? 'none' : networks.find(n => n.Id === args[2]).Name;
        delete c.NetworkSettings.Networks[name];
      } else if (args[0] === 'network' && args[1] === 'connect') {
        const network = networks.find(n => n.Id === args.at(-2)), aliases = [];
        for (let i = 0; i < args.length; i++) if (args[i] === '--alias') aliases.push(args[i + 1]);
        const c = containers.get(args.at(-1)), requested = args[args.indexOf('--ip') + 1];
        c.NetworkSettings.Networks[network.Name] = { NetworkID: c.State.Running ? network.Id : '', IPAddress: c.State.Running ? requested : '', IPAMConfig: { IPv4Address: requested }, Aliases: aliases };
      } else if (args[0] === 'create') {
        const replacement = structuredClone(before); replacement.Id = newId; replacement.Name = `/${contract.name}`; replacement.State.Running = false;
        replacement.Config.Labels = {};
        for (let i = 0; i < args.length; i++) if (args[i] === '--label') { const split = args[i + 1].indexOf('='); replacement.Config.Labels[args[i + 1].slice(0, split)] = args[i + 1].slice(split + 1); }
        replacement.Config.Env = fs.readFileSync(args[args.indexOf('--env-file') + 1], 'utf8').trimEnd().split('\n');
        replacement.Mounts = contract.mounts.map(m => ({ Type: 'bind', Source: m.source, Destination: m.destination, RW: m.writable }));
        replacement.NetworkSettings.Networks = { none: { NetworkID: 'none', IPAddress: '', Aliases: [] } }; containers.set(newId, replacement); result = newId;
      } else throw Error('unsupported_synthetic_command');
      if (fails && after) throw Error('synthetic_unconfirmed');
      return result;
    },
  };
  return { c: contract, docker, before, artifact };
}

test('all exact preview styles preserve private addresses and pin image bytes without credential argv', t => {
  for (const name of ['memory-company-20260919-gateway-1', 'memory-company-20260919-gateway_edge-1', 'memory-workshop-preview-app-20260918', 'memory-homelab-preview-app-20260919']) {
    const { c, docker } = fixture(t, name); const review = execute(c, 'review', undefined, docker);
    assert.equal(review.image_id, imageId);
    const result = execute(c, 'replace', review.review, docker);
    assert.equal(result.started, true); assert.equal(result.health_verified, false);
    assert.equal(docker.containers.get(oldId).Name, `/${c.retained_name}`);
    assert.equal(docker.containers.get(oldId).State.Running, false);
    assert.equal(Object.keys(docker.containers.get(oldId).NetworkSettings.Networks).length, 0);
    assert.equal(docker.containers.get(newId).NetworkSettings.Networks.first.IPAddress, '172.22.0.5');
    const command = docker.commands.find(a => a[0] === 'create');
    assert.ok(command.includes(imageId)); assert.ok(!command.includes('mutable:tag'));
    assert.ok(command.includes('--env-file')); assert.ok(!command.join(' ').includes('not-in-argv'));
    assert.ok(!docker.commands.some(a => ['rm', 'pull', 'exec'].includes(a[0])));
  }
});

test('unsupported entrypoint and weakened isolation fail before stopping anything', t => {
  for (const modify of [v => { v.Config.Entrypoint = ['shell', '-c']; }, v => { v.HostConfig.SecurityOpt = ['no-new-privileges:false']; },
    v => { v.HostConfig.PidMode = 'host'; }, v => { v.NetworkSettings.Ports = { '8080/tcp': [{ HostPort: '8080' }] }; },
    v => { v.Config.Env.push('SECRET=multiline\nvalue'); }]) {
    const { c, docker, before } = fixture(t); modify(before);
    assert.throws(() => execute(c, 'review', undefined, docker));
    assert.ok(!docker.commands.some(a => a[0] === 'stop')); assert.equal(before.State.Running, true);
  }
});

test('exact contract, backup and artifact changes fail without replacing a container', t => {
  const { c, docker, artifact } = fixture(t); const review = execute(c, 'review', undefined, docker);
  assert.throws(() => execute({ ...c, name: 'other-live-app' }, 'replace', review.review, docker), /invalid_target/);
  assert.throws(() => execute(c, 'replace', 'f'.repeat(64), docker), /exact_review/);
  fs.writeFileSync(artifact, 'changed'); assert.throws(() => execute(c, 'replace', review.review, docker), /artifact_changed/);
  assert.ok(!docker.commands.some(a => a[0] === 'stop'));
});

test('explicit recovery handles failures before rename, after disconnect, ambiguous create and after start', t => {
  for (const [predicate, after] of [
    [a => a[0] === 'stop', false], [a => a[0] === 'stop', true], [a => a[0] === 'rename', true],
    [a => a[0] === 'network' && a[1] === 'disconnect', true], [a => a[0] === 'create', true], [a => a[0] === 'start', true],
  ]) {
    const { c, docker, artifact } = fixture(t), review = execute(c, 'review', undefined, docker);
    docker.failOnce(predicate, after);
    assert.throws(() => execute(c, 'replace', review.review, docker), /synthetic_unconfirmed/);
    // No automatic restoration is attempted after an ambiguous external effect.
    assert.equal(fs.existsSync(path.join(c.directory, 'restored-old.json')), false);
    fs.unlinkSync(artifact); // A broken new artifact cannot prevent restoring the retained old container.
    const recovered = execute(c, 'restore-old', review.review, docker);
    assert.equal(recovered.remote_effects_reversed, false); assert.equal(recovered.health_verified, false);
    assert.equal(docker.containers.get(oldId).Name, `/${c.name}`); assert.equal(docker.containers.get(oldId).State.Running, true);
    assert.equal(Object.keys(docker.containers.get(oldId).NetworkSettings.Networks).length, 2);
    if (docker.containers.has(newId)) { assert.equal(docker.containers.get(newId).State.Running, false); assert.match(docker.containers.get(newId).Name, /unconfirmed-dependable$/); }
    assert.equal(execute(c, 'restore-old', review.review, docker).restored_old, oldId);
  }
});

test('recovery will not touch an unrecorded replacement with altered attribution or mounts', t => {
  const { c, docker } = fixture(t), review = execute(c, 'review', undefined, docker);
  docker.failOnce(a => a[0] === 'create', true);
  assert.throws(() => execute(c, 'replace', review.review, docker));
  docker.containers.get(newId).Config.Labels = {};
  const before = docker.commands.length;
  assert.throws(() => execute(c, 'restore-old', review.review, docker), /replacement_identity_changed/);
  assert.ok(!docker.commands.slice(before).some(a => ['stop', 'update', 'rename', 'start'].includes(a[0])));
});

test('active operation lock and false no-new-privileges policy fail closed', t => {
  const { c, docker, before } = fixture(t);
  fs.writeFileSync(path.join(c.directory, 'operation-lock.json'), '{"pid":1}', { mode: 0o600 });
  assert.throws(() => execute(c, 'review', undefined, docker), /owner_reconciliation/);
  before.HostConfig.SecurityOpt = ['no-new-privileges=false']; assert.throws(() => validateBoundary(before), /boundary/);
});

test('rc2 replacement retains the immediately prior container without touching historical retained containers', t => {
  const { c, docker, before } = fixture(t);
  c.retained_name = `${c.name}-before-rc2`;
  const historical = structuredClone(before), historicalId = 'd'.repeat(64);
  historical.Id = historicalId; historical.Name = `/${c.name}-before-dependable`; historical.State.Running = false;
  docker.containers.set(historicalId, historical);
  const review = execute(c, 'review', undefined, docker);
  assert.equal(execute(c, 'replace', review.review, docker).started, true);
  assert.equal(docker.containers.get(oldId).Name, `/${c.retained_name}`);
  assert.deepEqual(docker.containers.get(historicalId), historical);
  assert.ok(fs.existsSync(path.join(c.directory, 'start-intent.json')));
  assert.ok(!docker.commands.some(args => args.includes(historicalId)));
});

test('rc2 retained names remain narrowly scoped and bind the exact reviewed contract', t => {
  const { c, docker } = fixture(t);
  for (const suffix of ['before-rc3', 'before-rc2-extra', 'unconfirmed-rc2', '../elsewhere', 'before-RC2'])
    assert.throws(() => validate({ ...c, retained_name: `${c.name}-${suffix}` }), /invalid_recovery_directory/);
  const review = execute(c, 'review', undefined, docker);
  assert.throws(() => execute({ ...c, retained_name: `${c.name}-before-rc2` }, 'replace', review.review, docker), /exact_review_required/);
  assert.ok(!docker.commands.some(args => args[0] === 'stop'));
});

test('Mac onboarding replacement has a distinct retained name and forbids post-start rollback', t => {
  const { c, docker } = fixture(t, 'memory-homelab-preview-app-20260919');
  c.retained_name = `${c.name}-before-mac-onboarding-20261015`;
  const review = execute(c, 'review', undefined, docker);
  execute(c, 'replace', review.review, docker);
  assert.equal(docker.containers.get(oldId).Name, `/${c.retained_name}`);
  assert.ok(fs.existsSync(path.join(c.directory, 'start-intent.json')));
  assert.throws(() => execute(c, 'restore-old', review.review, docker), /post_start_requires_forward_recovery/);
  assert.throws(() => validate({ ...c, retained_name: `${c.name}-before-mac-onboarding-other` }), /invalid_recovery_directory/);
});

test('Mac project sync repair preserves the previous onboarding container and requires forward recovery', t => {
  const { c, docker } = fixture(t, 'memory-workshop-preview-app-20260918');
  c.retained_name = `${c.name}-before-mac-project-sync-20261015`;
  const review = execute(c, 'review', undefined, docker);
  execute(c, 'replace', review.review, docker);
  assert.equal(docker.containers.get(oldId).Name, `/${c.retained_name}`);
  assert.throws(() => execute(c, 'restore-old', review.review, docker), /post_start_requires_forward_recovery/);
});

test('rc2 pre-start recovery uses its own retained namespace', t => {
  const { c, docker } = fixture(t); c.retained_name = `${c.name}-before-rc2`;
  const review = execute(c, 'review', undefined, docker);
  docker.failOnce(args => args[0] === 'create', true);
  assert.throws(() => execute(c, 'replace', review.review, docker), /synthetic_unconfirmed/);
  assert.equal(execute(c, 'restore-old', review.review, docker).restored_old, oldId);
  assert.equal(docker.containers.get(newId).Name, `/${c.name}-unconfirmed-rc2`);
});

test('rc2 possible startup requires forward recovery even if start acknowledgement is lost', t => {
  for (const after of [false, true]) {
    const { c, docker } = fixture(t); c.retained_name = `${c.name}-before-rc2`;
    const review = execute(c, 'review', undefined, docker);
    docker.failOnce(args => args[0] === 'start', after);
    assert.throws(() => execute(c, 'replace', review.review, docker), /synthetic_unconfirmed/);
    const commandCount = docker.commands.length;
    assert.throws(() => execute(c, 'restore-old', review.review, docker), /post_start_requires_forward_recovery/);
    assert.equal(docker.commands.length, commandCount);
    assert.equal(docker.containers.get(oldId).State.Running, false);
  }
});
