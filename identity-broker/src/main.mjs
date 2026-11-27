// SPDX-License-Identifier: AGPL-3.0-or-later
import { NodeOAuthClient } from '@atproto/oauth-client-node';
import { EphemeralStore, IdentityBroker, metadata } from './broker.mjs';
import { createPublicFetch } from './network.mjs';
import { listenProtected } from './server.mjs';

const origin=process.env.CODIFIX_IDENTITY_PUBLIC_ORIGIN;
const socket=process.env.CODIFIX_IDENTITY_SOCKET;
if(!origin || !socket) throw new Error('CODIFIX_IDENTITY_PUBLIC_ORIGIN and CODIFIX_IDENTITY_SOCKET are required');
const stateStore=new EphemeralStore(),sessionStore=new EphemeralStore();
const fetch=createPublicFetch();
const client=new NodeOAuthClient({clientMetadata:metadata(origin),stateStore,sessionStore,fetch,allowHttp:false});
const broker=new IdentityBroker({client,stateStore,sessionStore,fetch});
const runtime=await listenProtected(socket,broker);
// No request URLs, callbacks, identities, authorization codes or tokens are logged.
const sweep=setInterval(()=>{broker.sweep();stateStore.sweep();sessionStore.sweep();},30000).unref();
for(const signal of ['SIGINT','SIGTERM']) process.once(signal,()=>{clearInterval(sweep);runtime.close().then(()=>process.exit(0),()=>process.exit(1));});
console.info('Codifix identity broker listening on protected Unix socket');
