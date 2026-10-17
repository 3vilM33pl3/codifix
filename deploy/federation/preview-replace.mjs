#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Exact reviewed replacement of a private preview container. No image pulls,
// schema operations, external ports, credential changes or automatic rollback.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const check = (v, code) => { if (!v) throw Error(code); };
const hash = value => createHash('sha256').update(value).digest('hex');
const hex = /^[a-f0-9]{64}$/;
const names = /^codifix-(?:company-20260919-(?:gateway-1|gateway_edge-1)|(?:workshop-preview-app-20260918|homelab-preview-app-20260919))$/;
function run(args) {
  const r = spawnSync('docker', args, { encoding: 'utf8', maxBuffer: 4 * 1024 * 1024, timeout: 55000 });
  check(r.status === 0, 'scoped_container_operation_unconfirmed'); return r.stdout.trim();
}
const label = 'dev.codifix.preview-replace.review';
const oldLabel = 'dev.codifix.preview-replace.old-id';
// Both schema-changing upgrades require forward recovery after start intent.
const rc2 = c => c.retained_name === `${c.name}-before-rc2`
  || c.retained_name === `${c.name}-before-mac-onboarding-20261015`
  || c.retained_name === `${c.name}-before-mac-project-sync-20261015`;
const unconfirmedName = c => `${c.name}-unconfirmed-${c.retained_name.endsWith('-mac-project-sync-20261015')
  ? 'mac-project-sync-20261015' : c.retained_name.endsWith('-mac-onboarding-20261015')
    ? 'mac-onboarding-20261015' : rc2(c) ? 'rc2' : 'dependable'}`;
const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const sorted = values => [...values].sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
function dir(p) {
  const s = fs.lstatSync(p); check(s.isDirectory() && s.uid === process.getuid() && !(s.mode & 0o077) && fs.realpathSync(p) === p, 'private_directory_required');
  for (let current = path.dirname(p);;) {
    const entry = fs.lstatSync(current);
    check(entry.isDirectory() && [0, process.getuid()].includes(entry.uid)
      && (!(entry.mode & 0o022) || (entry.mode & 0o1000)), 'untrusted_private_directory_ancestry');
    const parent = path.dirname(current); if (parent === current) break; current = parent;
  }
}
function read(p) { dir(path.dirname(p)); const s = fs.lstatSync(p); check(s.isFile() && s.uid === process.getuid() && s.nlink === 1 && !(s.mode & 0o077) && s.size < 4 * 1024 * 1024, 'private_file_required'); return fs.readFileSync(p); }
function syncParent(p) { const fd = fs.openSync(path.dirname(p), fs.constants.O_RDONLY); try { fs.fsyncSync(fd); } finally { fs.closeSync(fd); } }
function writeBytes(p, bytes) { dir(path.dirname(p)); const fd = fs.openSync(p, fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL | fs.constants.O_NOFOLLOW, 0o600); try { fs.writeFileSync(fd, bytes); fs.fsyncSync(fd); } finally { fs.closeSync(fd); } syncParent(p); }
function write(p, value) { writeBytes(p, JSON.stringify(value, null, 2) + '\n'); }
export function artifactHash(p) {
  const h = createHash('sha256');
  function visit(p, relative) {
    const s = fs.lstatSync(p); check(s.uid === process.getuid() && !(s.mode & 0o022) && !s.isSymbolicLink(), 'unsafe_artifact');
    if (s.isDirectory()) {
      h.update(`directory:${relative}\n`);
      for (const name of fs.readdirSync(p).sort()) visit(path.join(p, name), `${relative}/${name}`);
    } else {
      check(s.isFile() && s.nlink === 1, 'unsafe_artifact');
      h.update(`file:${relative}:${s.size}:`);
      const fd = fs.openSync(p, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW), bytes = Buffer.alloc(1024 * 1024);
      try { for (;;) { const n = fs.readSync(fd, bytes); if (!n) break; h.update(bytes.subarray(0, n)); } } finally { fs.closeSync(fd); }
    }
  }
  visit(p, ''); return h.digest('hex');
}
export function validate(c, checkPaths = true) {
  check(c.schema_version === 1 && hex.test(c.old_id) && names.test(c.name), 'invalid_target');
  check([`${c.name}-before-dependable`, `${c.name}-before-rc2`, `${c.name}-before-mac-onboarding-20261015`, `${c.name}-before-mac-project-sync-20261015`].includes(c.retained_name)
    && path.isAbsolute(c.directory) && path.basename(c.directory).startsWith('replace-'), 'invalid_recovery_directory');
  check(path.isAbsolute(c.backup_receipt) && hex.test(c.backup_receipt_sha256), 'backup_binding_required');
  check(Array.isArray(c.mounts) && c.mounts.length > 0 && new Set(c.mounts.map(m => m.destination)).size === c.mounts.length, 'invalid_mount_plan');
  for (const m of c.mounts) {
    check(path.isAbsolute(m.source) && (!checkPaths || fs.realpathSync(m.source) === m.source) && path.isAbsolute(m.destination)
      && path.normalize(m.destination) === m.destination && m.destination !== '/' && !/[\r\n\0,]/.test(m.source + m.destination)
      && typeof m.writable === 'boolean', 'invalid_mount');
    check(m.preserve === true ? m.sha256 === undefined : hex.test(m.sha256) && m.writable === false, 'reviewed_artifact_required');
  }
  return c;
}
export function validateBoundary(v) {
  check(v.Config.User === '1000:1000' && v.HostConfig.ReadonlyRootfs && !v.HostConfig.Privileged && v.HostConfig.CapDrop?.includes('ALL')
    && v.HostConfig.SecurityOpt?.some(o => ['no-new-privileges', 'no-new-privileges:true', 'no-new-privileges=true'].includes(o))
    && !Object.keys(v.HostConfig.PortBindings ?? {}).length && !v.HostConfig.PublishAllPorts
    && v.HostConfig.PidMode !== 'host' && v.HostConfig.IpcMode !== 'host'
    && !v.HostConfig.CapAdd?.length && !v.HostConfig.Devices?.length && !v.HostConfig.DeviceRequests?.length
    && v.Mounts.every(m => m.Type === 'bind'), 'preview_boundary_changed');
  check(!Object.values(v.NetworkSettings.Ports ?? {}).some(v => v?.length), 'published_ports_forbidden');
  check(/^sha256:[a-f0-9]{64}$/.test(v.Image), 'immutable_image_required');
}
function restart(host) {
  const r = host.RestartPolicy;
  check(r && ['no', 'always', 'unless-stopped', 'on-failure'].includes(r.Name) && Number.isSafeInteger(r.MaximumRetryCount) && r.MaximumRetryCount >= 0, 'unsupported_restart_policy');
  return r.Name === 'on-failure' && r.MaximumRetryCount ? `on-failure:${r.MaximumRetryCount}` : r.Name;
}
function snapshot(c, docker) {
  const v = docker.inspect(c.old_id);
  check(v.Id === c.old_id && v.Name === `/${c.name}` && v.State?.Running, 'container_identity_changed');
  validateBoundary(v);
  check((v.Config.Entrypoint ?? []).length <= 1, 'unsupported_entrypoint');
  check(Array.isArray(v.Config.Env) && v.Config.Env.every(s => /^[A-Za-z_][A-Za-z0-9_]*=/.test(s) && !/[\r\n\0]/.test(s))
    && new Set(v.Config.Env.map(s => s.split('=', 1)[0])).size === v.Config.Env.length, 'unsupported_environment');
  check(Number.isSafeInteger(v.HostConfig.Memory) && v.HostConfig.Memory > 0 && Number.isSafeInteger(v.HostConfig.PidsLimit) && v.HostConfig.PidsLimit > 0, 'bounded_resources_required');
  check(!(v.HostConfig.NanoCpus || v.HostConfig.CpuQuota || v.HostConfig.CpusetCpus || v.HostConfig.CpusetMems), 'unsupported_resource_override');
  restart(v.HostConfig);
  for (const m of c.mounts) if (m.preserve) {
    if (m.destination === '/gateway-access/current') {
      check(c.name === 'codifix-company-20260919-gateway-1' && !m.writable && m.source === '/home/olivier/.local/state/codifix-company-20260919/refresh-maintenance/access', 'access_only_mount_required'); dir(m.source);
    } else check(v.Mounts.some(old => old.Source === m.source && old.Destination === m.destination && old.RW === m.writable), 'preserved_mount_changed');
  }
  const networks = Object.entries(v.NetworkSettings.Networks).sort(([a], [b]) => a.localeCompare(b)).map(([name, n]) => {
    const actual = JSON.parse(docker.run(['network', 'inspect', n.NetworkID]))[0];
    check(hex.test(n.NetworkID) && actual.Id === n.NetworkID && actual.Name === name && actual.Internal && !actual.EnableIPv6, 'network_boundary_changed');
    check(/^(?:\d{1,3}\.){3}\d{1,3}$/.test(n.IPAddress) && n.IPAddress.split('.').every(part => Number(part) <= 255)
      && n.IPAddress !== '0.0.0.0' && (n.Aliases ?? []).every(alias => /^[A-Za-z0-9][A-Za-z0-9_.-]*$/.test(alias)), 'invalid_network_address_or_alias');
    return { name, id: n.NetworkID, address: n.IPAddress, aliases: n.Aliases ?? [] };
  });
  check(networks.length >= 2 && networks.length <= 4, 'unexpected_networks');
  return { id: v.Id, name: c.name, image_id: v.Image, config: v.Config, host: v.HostConfig, mounts: sorted(v.Mounts.map(m => [m.Source,m.Destination,m.RW])), networks };
}
function verifyFiles(c, artifacts = true) {
  check(hash(read(c.backup_receipt)) === c.backup_receipt_sha256, 'fresh_backup_receipt_changed');
  if (artifacts) for (const m of c.mounts) if (!m.preserve) check(artifactHash(m.source) === m.sha256, 'reviewed_artifact_changed');
}
function connect(network, id, docker) {
  const args = ['network', 'connect', '--ip', network.address];
  for (const alias of network.aliases) args.push('--alias', alias);
  docker.run([...args, network.id, id]);
}
function named(name, docker) { const id = docker.run(['ps', '-aq', '--no-trunc', '--filter', `name=^/${name}$`]); check(!id || hex.test(id), 'ambiguous_named_container'); return id || null; }
function checkNetworks(v, old, docker, complete = false) {
  const current = Object.entries(v.NetworkSettings.Networks);
  for (const [name, value] of current) {
    if (name === 'none' && !complete) continue;
    const n = old.networks.find(n => n.name === name);
    check(n && equal(sorted(value.Aliases ?? []), sorted(n.aliases)), 'container_networks_changed');
    if (!v.State.Running && !value.IPAddress) {
      // Docker records requested networking before start, but assigns endpoint
      // IDs and addresses only when the stopped container actually starts.
      const network = JSON.parse(docker.run(['network', 'inspect', name]))[0];
      check(network.Id === n.id && network.Name === name && network.Internal && !network.EnableIPv6
        && (!value.NetworkID || value.NetworkID === n.id), 'container_networks_changed');
      const requested = value.IPAMConfig?.IPv4Address;
      check(requested ? requested === n.address : v.Id === old.id, 'requested_container_address_changed');
    } else check(value.NetworkID === n.id && value.IPAddress === n.address, 'container_networks_changed');
  }
  if (complete) check(current.length === old.networks.length, 'container_networks_incomplete');
}
function createdBoundary(v, c, old, approval, docker) {
  validateBoundary(v);
  check(v.Image === old.image_id && [`/${c.name}`, `/${unconfirmedName(c)}`].includes(v.Name)
    && v.Config.Labels?.[label] === approval && v.Config.Labels?.[oldLabel] === c.old_id, 'replacement_identity_changed');
  check(equal(sorted(v.Config.Env ?? []), sorted(old.config.Env ?? [])) && equal(v.Config.Cmd ?? [], old.config.Cmd ?? [])
    && equal(v.Config.Entrypoint ?? [], old.config.Entrypoint ?? []) && v.Config.WorkingDir === old.config.WorkingDir
    && v.HostConfig.Memory === old.host.Memory && v.HostConfig.PidsLimit === old.host.PidsLimit
    && equal(sorted(v.Mounts.map(m => [m.Source, m.Destination, m.RW])), sorted(c.mounts.map(m => [m.source, m.destination, m.writable]))), 'replacement_contract_changed');
  checkNetworks(v, old, docker);
}
function retainedBoundary(v, c, old, docker) {
  validateBoundary(v);
  check(v.Id === c.old_id && [`/${c.name}`, `/${c.retained_name}`].includes(v.Name) && v.Image === old.image_id
    && equal(v.Config, old.config) && equal(sorted(v.Mounts.map(m => [m.Source, m.Destination, m.RW])), sorted(old.mounts)), 'retained_container_changed');
  checkNetworks(v, old, docker);
}
export function execute(c, phase, approval, injected) {
  check(['review', 'replace', 'restore-old'].includes(phase), 'explicit_phase_required');
  const docker = injected ?? { run, inspect: id => JSON.parse(run(['inspect', id]))[0] };
  validate(c, phase !== 'restore-old'); verifyFiles(c, phase !== 'restore-old'); dir(c.directory);
  const lock = `${c.directory}/operation-lock.json`;
  check(!fs.existsSync(lock), 'operation_lock_requires_owner_reconciliation');
  write(lock, { pid: process.pid, phase, review: approval ?? null });
  try { return executeLocked(c, phase, approval, docker); }
  finally { fs.unlinkSync(lock); syncParent(lock); }
}
function executeLocked(c, phase, approval, docker) {
  if (phase === 'review') {
    const before = snapshot(c, docker), review = hash(JSON.stringify({ c, before }));
    write(`${c.directory}/review.json`, { review, before, contract: c });
    return { review, target: c.name, old_id: c.old_id, image_id: before.image_id, mounts: c.mounts.map(m => m.destination), preserves_addresses: true, provider_calls: 0 };
  }
  const saved = JSON.parse(read(`${c.directory}/review.json`));
  check(hex.test(approval) && saved.review === approval && hash(JSON.stringify({ c, before: saved.before })) === approval, 'exact_review_required');
  const old = saved.before;
  if (phase === 'replace') {
    check(hash(JSON.stringify(snapshot(c, docker))) === hash(JSON.stringify(old)), 'preview_changed_since_review');
    check(!fs.existsSync(`${c.directory}/intent.json`), 'existing_intent_requires_reconciliation');
    check(!named(c.retained_name, docker) && !named(unconfirmedName(c), docker), 'retained_name_already_in_use');
    check(JSON.parse(docker.run(['image', 'inspect', old.image_id]))[0].Id === old.image_id, 'immutable_image_unavailable');
    const environment = `${c.directory}/environment.list`;
    writeBytes(environment, old.config.Env.join('\n') + '\n');
    write(`${c.directory}/intent.json`, { review: approval, old_id: c.old_id, created_at: new Date().toISOString() });
    docker.run(['stop', '--time', '30', c.old_id]);
    docker.run(['update', '--restart=no', c.old_id]);
    docker.run(['rename', c.old_id, c.retained_name]);
    for (const n of old.networks) docker.run(['network', 'disconnect', n.id, c.old_id]);
    const args = ['create', '--pull=never', '--name', c.name, '--restart', restart(old.host), '--network', 'none', '--user', old.config.User,
      '--label', `${label}=${approval}`, '--label', `${oldLabel}=${c.old_id}`, '--env-file', environment,
      '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges', '--read-only', '--pids-limit', String(old.host.PidsLimit), '--memory', String(old.host.Memory)];
    if (old.config.WorkingDir) args.push('--workdir', old.config.WorkingDir);
    for (const [p, value] of Object.entries(old.host.Tmpfs ?? {})) args.push('--tmpfs', `${p}:${value}`);
    for (const m of c.mounts) args.push('--mount', `type=bind,src=${m.source},dst=${m.destination}${m.writable ? '' : ',readonly'}`);
    check((old.config.Entrypoint ?? []).length <= 1, 'unsupported_entrypoint');
    if (old.config.Entrypoint?.length) args.push('--entrypoint', old.config.Entrypoint[0]);
    args.push(old.image_id, ...(old.config.Cmd ?? []));
    const id = docker.run(args); check(hex.test(id), 'creation_unconfirmed');
    write(`${c.directory}/created.json`, { review: approval, id, old_id: c.old_id });
    createdBoundary(docker.inspect(id), c, old, approval, docker);
    docker.run(['network', 'disconnect', 'none', id]);
    for (const n of old.networks) connect(n, id, docker);
    const ready = docker.inspect(id); createdBoundary(ready, c, old, approval, docker); checkNetworks(ready, old, docker, true);
    // rc.2 may migrate the database or publish immediately at startup. Once a
    // start is possible, the retained older executable is no longer a safe
    // rollback even if Docker's acknowledgement is lost.
    if (rc2(c)) write(`${c.directory}/start-intent.json`, { review: approval, id, old_id: c.old_id });
    docker.run(['start', id]);
    const started = docker.inspect(id); createdBoundary(started, c, old, approval, docker); checkNetworks(started, old, docker, true);
    check(started.State.Running, 'replacement_start_unconfirmed');
    write(`${c.directory}/started.json`, { review: approval, id, health_verified: false, provider_calls: 0 });
    return { started: true, id, retained: c.old_id, health_verified: false, provider_calls: 0 };
  }
  if (phase === 'restore-old') {
    const intent = JSON.parse(read(`${c.directory}/intent.json`));
    check(intent.review === approval && intent.old_id === c.old_id, 'replacement_intent_required');
    if (rc2(c)) check(!fs.existsSync(`${c.directory}/start-intent.json`)
      && !fs.existsSync(`${c.directory}/started.json`), 'post_start_requires_forward_recovery');
    let prior = docker.inspect(c.old_id); retainedBoundary(prior, c, old, docker);
    let replacement;
    if (fs.existsSync(`${c.directory}/created.json`)) {
      const created = JSON.parse(read(`${c.directory}/created.json`));
      check(created.review === approval && created.old_id === c.old_id && hex.test(created.id) && created.id !== c.old_id, 'creation_receipt_changed');
      replacement = created.id;
    } else {
      const candidate = named(c.name, docker) ?? named(unconfirmedName(c), docker);
      if (candidate && candidate !== c.old_id) replacement = candidate;
    }
    if (replacement) {
      const live = docker.inspect(replacement); createdBoundary(live, c, old, approval, docker);
      if (rc2(c)) check(!live.State.Running, 'post_start_requires_forward_recovery');
      if (!fs.existsSync(`${c.directory}/created.json`) && !fs.existsSync(`${c.directory}/reconciled-created.json`))
        write(`${c.directory}/reconciled-created.json`, { review: approval, id: replacement, old_id: c.old_id, observed_only: true });
      if (live.State.Running) docker.run(['stop', '--time', '30', replacement]);
      docker.run(['update', '--restart=no', replacement]);
      for (const n of old.networks) if (live.NetworkSettings.Networks[n.name]) docker.run(['network', 'disconnect', n.id, replacement]);
      if (live.Name === `/${c.name}`) {
        check(!named(unconfirmedName(c), docker), 'unconfirmed_name_already_in_use');
        docker.run(['rename', replacement, unconfirmedName(c)]);
      }
    }
    prior = docker.inspect(c.old_id); retainedBoundary(prior, c, old, docker);
    for (const n of old.networks) if (!prior.NetworkSettings.Networks[n.name]) connect(n, c.old_id, docker);
    if (prior.Name === `/${c.retained_name}`) docker.run(['rename', c.old_id, c.name]);
    docker.run(['update', `--restart=${restart(old.host)}`, c.old_id]);
    if (!prior.State.Running) docker.run(['start', c.old_id]);
    const restored = docker.inspect(c.old_id); retainedBoundary(restored, c, old, docker); checkNetworks(restored, old, docker, true);
    check(restored.Name === `/${c.name}` && restored.State.Running, 'restoration_unconfirmed');
    const result = { review: approval, restored_old: c.old_id, health_verified: false, remote_effects_reversed: false };
    if (!fs.existsSync(`${c.directory}/restored-old.json`)) write(`${c.directory}/restored-old.json`, result);
    return result;
  }
  throw Error('explicit_phase_required');
}
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  process.umask(0o077);
  try { const [file, phase, approval] = process.argv.slice(2); console.log(JSON.stringify(execute(JSON.parse(read(file)), phase, approval))); }
  catch (e) { console.error(JSON.stringify({ error: /^[a-z_]+$/.test(e.message) ? e.message : 'private_operation_unconfirmed', automatic_rollback: false, recovery: 'Inspect private intent and exact container identities before retrying. For rc2, any start-intent requires forward recovery with the reviewed candidate; restore-old is only available before possible startup. A crash-held operation lock requires explicit owner reconciliation; never remove it while its process runs.' })); process.exitCode = 1; }
}
