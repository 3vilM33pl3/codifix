// SPDX-License-Identifier: AGPL-3.0-or-later
// Trusted launcher building block. It binds the exact approved campaign to the
// existing disposable preparation/execution session, without exposing its key.
// This does not change the CLI's paid-dispatch guard or approve a campaign.
import path from 'node:path';
import { withApprovedProvider } from './approved-provider.mjs';
import { openProviderCampaignSession } from './provider-campaign-session.mjs';

export async function withApprovedCampaignSession(options, use) {
  const { credential, session } = structuredClone(options);
  if (typeof use !== 'function' || !credential || !session
      || session.offline !== false || session.provider !== undefined
      || !session.configuration || !path.isAbsolute(session.configuration.root)
      || ![session.configuration.nodeImage, session.configuration.rustImage].every(image => /^sha256:[a-f0-9]{64}$/.test(image))
      || credential.manifest !== path.join(session.configuration.root, 'campaign.json')
      || credential.manifestSha256 !== session.configuration.manifestSha256
      || session.split !== credential.split?.replace('-', '_')) throw Error('approved session identity mismatch');
  return withApprovedProvider(credential, async provider => {
    const active = await openProviderCampaignSession({ ...session, provider });
    try { await use(active); }
    finally { await active.close(); }
  });
}
