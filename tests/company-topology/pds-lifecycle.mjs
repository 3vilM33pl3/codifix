// SPDX-License-Identifier: AGPL-3.0-or-later
// Called only by the disposable topology harness, which owns all resources.
import { mkdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import assert from 'node:assert/strict';

export async function rehearsePds(ctx) {
  assert.ok(ctx.root && ctx.stateDir.startsWith(`${ctx.root}/`));
  const prepared = await ctx.runOperator(['prepare', ctx.domain, ctx.repositoryHostname, ctx.operatorHostname]);
  assert.equal(prepared.synthetic, true);
  const bootstrap = await ctx.runOperator(['phase', 'bootstrap']);
  assert.equal(bootstrap.result.state, 'bootstrap');
  await ctx.replaceDidDocument(await readFile(join(ctx.stateDir, 'state/did.json'), 'utf8'));
  const data = join(ctx.root, 'actual-pds-data');
  await mkdir(data, { mode: 0o700 });
  await ctx.startRealPds(join(ctx.stateDir, 'pds.env'), data);
  const invited = await ctx.runOperator(['invite']);
  assert.equal(invited.invitation_persisted, true);
  const created = await ctx.runOperator(['phase', 'create']);
  assert.equal(created.result.state, 'created');
  const discovered = await ctx.runOperator(['phase', 'discover-key']);
  assert.equal(discovered.result.state, 'key_review');
  const handoff = await ctx.runOperator(['phase', 'handoff']);
  assert.equal(handoff.result.state, 'handoff');
  await ctx.replaceDidDocument(await readFile(join(ctx.stateDir, 'state/did.json'), 'utf8'));
  const active = await ctx.runOperator(['phase', 'activate']);
  assert.equal(active.result.state, 'active');
  const refreshed = await ctx.runOperator(['phase', 'refresh']);
  assert.equal(refreshed.result.session_generation, 2);
  const verified = await ctx.runPublisher(['publish-verify']);
  assert.equal(verified.repository_verified, true); assert.equal(verified.wrong_key_rejected, true);
  return { synthetic: true, image: prepared.image, sdk_version: prepared.sdk_version,
    created: true, recommended_key_reviewed: true, did_handoff_verified: true,
    active: true, session_refreshed: true, signed_repository_verified: true,
    session_generation: refreshed.result.session_generation,
    wrong_key_rejected: true, repository_records: verified.records, car_bytes: verified.car_bytes,
    gateway_enrolled: false, customer_data: false, paid_calls: 0 };
}
