// SPDX-License-Identifier: AGPL-3.0-or-later
// Linux Docker fixture only. All secrets, containers and volumes are disposable.
import {mkdtemp,mkdir,readFile,writeFile,copyFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';import {join,dirname,resolve} from 'node:path';
import {randomBytes,randomUUID,createECDH} from 'node:crypto';
import {spawnSync,spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {topology,tlsProxy} from '../../deploy/federation/topology.mjs';
const repo=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const root=await mkdtemp(join(tmpdir(),'memory-isolation-'));
const project=`memory-isolation-${randomBytes(6).toString('hex')}`;
const pdsImage='ghcr.io/bluesky-social/pds@sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12';
const postgresImage='pgvector/pgvector@sha256:cf134a767f474095eeba57e0117be8e568e011a63f33fbf252f14c9b760f8e6f';
const proxyImage='caddy@sha256:226d1f059b75399fe19182893c7184591c07b97afc8dfcf44eeb80c9a77a530f';
const image=`${project}:fixture`,proxy=`${project}:proxy`;const compose=['compose','-p',project,'-f',join(root,'compose.json')];
function run(command,args,options={}) {
  const result=spawnSync(command,args,{cwd:repo,encoding:'utf8',timeout:60_000,...options});
  if(result.status!==0) {
    const safe=(result.stderr||'').replace(/[a-f0-9]{64}/gi,'[fixture-secret]').replace(/postgres(?:ql)?:\/\/\S+/g,'[database-url]');
    throw new Error(`${command} ${args[0]} failed with exit ${result.status}: ${safe.slice(0,3000)}`);
  }
  return result.stdout?.trim() || '';
}
async function long(command,args,options={}) {
  const child=spawn(command,args,{cwd:repo,stdio:'inherit',...options});
  const timer=setTimeout(()=>child.kill('SIGTERM'),300_000);
  try {if(await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',resolve);})!==0) throw new Error(`${command} failed`);}
  finally {clearTimeout(timer);}
}
const secret=()=>randomBytes(32).toString('hex');
const save=(path,value)=>writeFile(path,typeof value==='string'?value:JSON.stringify(value),{mode:0o600});
function inspect(service) {
  const id=run('docker',[...compose,'ps','-aq',service]);
  const live=JSON.parse(run('docker',['inspect',id]))[0];
  if(live.State.Status==='exited') {
    const output=spawnSync('docker',['logs',id],{encoding:'utf8'});
    const safe=(output.stdout+output.stderr).replace(/[a-f0-9]{64}/gi,'[fixture-secret]').replace(/postgres(?:ql)?:\/\/\S+/g,'[database-url]');
    throw new Error(`${service} exited: ${safe.slice(0,5000)}`);
  }
  return live;
}
function ip(service) {return Object.values(inspect(service).NetworkSettings.Networks)[0].IPAddress;}
let started=false,built=false,proxyBuilt=false;
try {
  await long('cargo',['build','-p','mem-federation-runtime','--bin','memory-federation-gateway','--example','isolation-admin','--locked']);
  // A fixture image from our built binary and its exact dynamic libraries avoids
  // host mounts and dependence on a base image's glibc version. Not a release image.
  const staging=join(root,'image/rootfs');await mkdir(join(staging,'usr/local/bin'),{recursive:true});
  const binary=join(repo,'target/debug/memory-federation-gateway');
  await copyFile(binary,join(staging,'usr/local/bin/memory-federation-gateway'));
  const libs=[...run('ldd',[binary]).matchAll(/\/[\w/+.,:@-]+/g)].map(m=>m[0]);
  for(const lib of new Set(libs)) {await mkdir(dirname(join(staging,lib)),{recursive:true});await copyFile(lib,join(staging,lib));}
  await save(join(root,'image/Dockerfile'),'FROM scratch\nCOPY rootfs /\nENTRYPOINT ["/usr/local/bin/memory-federation-gateway"]\n');
  await long('docker',['build','-q','-t',image,join(root,'image')]);built=true;
  await copyFile(join(repo,'deploy/federation/Proxy.Dockerfile'),join(root,'image/Proxy.Dockerfile'));
  await long('docker',['build','-q','-t',proxy,'--build-arg',`PROXY_IMAGE=${proxyImage}`,'-f',join(root,'image/Proxy.Dockerfile'),join(root,'image')]);proxyBuilt=true;
  const cert=join(root,'cert.pem'),key=join(root,'key.pem'),ca=join(root,'ca.pem'),caKey=join(root,'ca-key.pem');
  run('openssl',['req','-x509','-newkey','rsa:2048','-nodes','-days','1','-keyout',caKey,'-out',ca,'-subj','/CN=Disposable Memory isolation CA','-addext','basicConstraints=critical,CA:TRUE']);
  run('openssl',['req','-new','-newkey','rsa:2048','-nodes','-keyout',key,'-out',join(root,'server.csr'),'-subj','/CN=gateway-a.federation.example.com']);
  await save(join(root,'leaf.ext'),'basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:gateway-a.federation.example.com,DNS:gateway-b.federation.example.com,DNS:pds-tls-a.federation.example.com,DNS:pds-tls-b.federation.example.com\n');
  run('openssl',['x509','-req','-in',join(root,'server.csr'),'-CA',ca,'-CAkey',caKey,'-CAcreateserial','-out',cert,'-days','1','-extfile',join(root,'leaf.ext')]);
  const metadata={};
  for(const id of ['a','b']) {
    const dir=join(root,id);for(const sub of ['gateway','edge','pds-tls','member']) await mkdir(join(dir,sub),{recursive:true});
    metadata[id]={space:randomUUID(),authority:randomUUID(),member:randomUUID(),adminPassword:secret(),gatewayPassword:secret(),memberToken:secret(),rootToken:secret()};
    const m=metadata[id];await save(join(dir,'metadata.json'),{space:m.space,authority:m.authority,member:m.member});
    for(const [file,value] of Object.entries({'gateway-db-password':m.adminPassword,'instance-db-password':secret(),'gateway-password':m.gatewayPassword,
      'gateway/audit-key':secret(),'gateway/pds-token':secret(),'gateway/member-token':m.memberToken,'gateway/root-token':m.rootToken,'member/token':m.memberToken})) await save(join(dir,file),value);
    await copyFile(ca,join(dir,'member/ca.pem'));await copyFile(ca,join(dir,'gateway/ca.pem'));
    for(const sub of ['edge','pds-tls']) {await copyFile(cert,join(dir,sub,'cert.pem'));await copyFile(key,join(dir,sub,'key.pem'));}
    await save(join(dir,'edge/Caddyfile'),tlsProxy(`gateway-${id}.federation.example.com`,`gateway_${id}:8081`,['/v1/accept','/v1/pull','/v1/retention','/v1/expertise']));
    await save(join(dir,'pds-tls/Caddyfile'),tlsProxy(`pds-tls-${id}.federation.example.com`,`pds_${id}:2583`,['/xrpc/com.atproto.repo.getRecord','/xrpc/com.atproto.repo.createRecord','/xrpc/com.atproto.sync.getRepo']));
    const pdsKey=createECDH('secp256k1');pdsKey.generateKeys();
    await save(join(dir,'pds.env'),Object.entries({PDS_PORT:'2583',PDS_HOSTNAME:`pds-${id}.federation.example.com`,PDS_DATA_DIRECTORY:'/pds',PDS_BLOBSTORE_DISK_LOCATION:'/pds/blobs',
      PDS_JWT_SECRET:secret(),PDS_ADMIN_PASSWORD:secret(),PDS_PLC_ROTATION_KEY_K256_PRIVATE_KEY_HEX:pdsKey.getPrivateKey('hex'),PDS_INVITE_REQUIRED:'true',PDS_DEV_MODE:'false',PDS_DISABLE_SSRF_PROTECTION:'false',PDS_CRAWLERS:'',LOG_LEVEL:'warn'}).map(([k,v])=>`${k}=${v}`).join('\n'));
  }
  await copyFile(join(repo,'tests/federation-isolation/probe.cjs'),join(root,'probe.cjs'));
  const doc=topology({gatewayImage:image,pdsImage,postgresImage,proxyImage:proxy,root,uid:process.getuid()});
  await save(join(root,'compose.json'),doc);
  for(const s of Object.values(doc.services)) {if(s.ports||s.privileged||s.network_mode) throw new Error('forbidden exposure');}
  started=true;run('docker',[...compose,'up','-d','gateway_db_a','gateway_db_b','instance_db_a','instance_db_b','pds_a','pds_b','pds_tls_a','pds_tls_b']);
  for(const id of ['a','b']) {
    const m=metadata[id],dir=join(root,id);
    for(let attempt=0;;attempt++) {
      try {run('docker',[...compose,'exec','-T',`gateway_db_${id}`,'pg_isready','-U','fixture_admin','-d','memory_isolation']);break;} catch {if(attempt>60) throw new Error('DB readiness');await new Promise(r=>setTimeout(r,250));}
    }
    await save(join(dir,'admin-url'),`postgres://fixture_admin:${m.adminPassword}@${ip(`gateway_db_${id}`)}:5432/memory_isolation`);
    const publicKeys=JSON.parse(run(join(repo,'target/debug/examples/isolation-admin'),[dir],{env:{...process.env,MEMORY_FEDERATION_ISOLATION_DISPOSABLE:'1'}}));
    await save(join(dir,'gateway/database-url'),`postgres://gateway_runtime:${m.gatewayPassword}@gateway_db_${id}:5432/memory_isolation`);
    await save(join(dir,'gateway/gateway.json'),{space:m.space,authority:m.authority,listen:'0.0.0.0:8081',database_url_file:'/config/database-url',pds:`https://pds-tls-${id}.federation.example.com:8443`,pds_ca_file:'/config/ca.pem',repository:`did:web:space-${id}.federation.example.com`,signing_key:publicKeys.pds_signing_key,
      token_file:'/config/pds-token',audit_key_file:'/config/audit-key',receipt_keys:publicKeys.receipt_keys,policy:{external_models:[]},expertise_fleet:[[m.member,3],[m.authority,3]],credentials:[{instance:m.member,token_file:'/config/member-token'},{instance:m.authority,token_file:'/config/root-token'}]});
  }
  run('docker',[...compose,'up','-d','gateway_a','gateway_b','edge_a','edge_b']);
  for(const id of ['a','b']) {
    const other=id==='a'?'b':'a';const denied=[];
    for(const s of ['a','b']) for(const [service,port] of [[`pds_${s}`,2583],[`pds_tls_${s}`,8443],[`gateway_${s}`,8081],[`gateway_db_${s}`,5432]]) {
      // Probe every interface by IP, not only names that DNS may hide.
      for(const net of Object.values(inspect(service).NetworkSettings.Networks)) denied.push({host:net.IPAddress,port});
      denied.push({host:service,port});
    }
    denied.push({host:ip(`instance_db_${other}`),port:5432});
    await save(join(root,id,'member/probe.json'),{id,space:metadata[id].space,gateway:`gateway-${id}.federation.example.com`,otherGateway:`gateway-${other}.federation.example.com`,denied,localDatabase:ip(`instance_db_${id}`)});
    run('docker',[...compose,'run','--rm','--no-deps','--use-aliases',`member_${id}`],{stdio:'inherit'});
    console.log(run(join(repo,'target/debug/examples/isolation-admin'),[join(root,id),'verify'],{env:{...process.env,MEMORY_FEDERATION_ISOLATION_DISPOSABLE:'1'}}));
  }
  for(const service of Object.keys(doc.services).filter(s=>!s.startsWith('member_'))) {
    const live=inspect(service);
    if(live.HostConfig.Privileged||Object.values(live.NetworkSettings.Ports||{}).some(Boolean)) throw new Error('live exposed port');
  }
  for(const name of Object.keys(doc.networks)) {
    const network=JSON.parse(run('docker',['network','inspect',`${project}_${name}`]))[0];
    if(!network.Internal || network.EnableIPv6) throw new Error('unexpected network exposure/IP-family contract');
  }
  console.log('PASS dedicated gateways: separate PDS and databases, no host ports, unchanged upstream security flags; no customer deployment');
} catch(error) {
  for(const service of ['gateway_a','edge_a','instance_db_a']) {
    const output=spawnSync('docker',[...compose,'logs','--tail','20',service],{encoding:'utf8'});
    console.error((output.stdout||'').replace(/[a-f0-9]{64}/gi,'[fixture-secret]').replace(/postgres(?:ql)?:\/\/\S+/g,'[database-url]'));
  }
  throw error;
} finally {
  try {if(started) run('docker',[...compose,'down','--volumes','--remove-orphans'],{stdio:'ignore'});}
  finally {try {if(built) run('docker',['image','rm',image],{stdio:'ignore'});if(proxyBuilt) run('docker',['image','rm',proxy],{stdio:'ignore'});} finally {await rm(root,{recursive:true,force:true});}}
}
