// SPDX-License-Identifier: AGPL-3.0-or-later
// Pure Compose model. Generating it does not provision, expose or deploy a host.
// Repeat a space unit for each authority; do not reuse its networks or secrets.
export function topology({ gatewayImage, pdsImage, postgresImage, proxyImage, root, uid }) {
  const doc = {services:{},networks:{members:{internal:true}},volumes:{}};
  const hardening = {read_only:true,cap_drop:['ALL'],security_opt:['no-new-privileges:true']};
  for (const id of ['a','b']) {
    for (const part of ['api','repository','database','local']) doc.networks[`${id}_${part}`]={internal:true};
    doc.volumes[`${id}_gateway_db`]={};doc.volumes[`${id}_instance_db`]={};doc.volumes[`${id}_pds`]={};
    doc.services[`gateway_${id}`]={image:gatewayImage,...hardening,user:String(uid),
      command:[`/config/gateway.json`],volumes:[`${root}/${id}/gateway:/config:ro`],
      networks:[`${id}_api`,`${id}_repository`,`${id}_database`]};
    doc.services[`edge_${id}`]={image:proxyImage,...hardening,user:String(uid),
      command:['caddy','run','--config','/config/Caddyfile','--adapter','caddyfile'],
      volumes:[`${root}/${id}/edge:/config:ro`],tmpfs:[`/data:uid=${uid},gid=${uid},mode=0700`,`/config-data:uid=${uid},gid=${uid},mode=0700`],environment:{XDG_CONFIG_HOME:'/config-data'},
      networks:{members:{aliases:[`gateway-${id}.federation.example.com`]},[`${id}_api`]:{}}};
    doc.services[`pds_tls_${id}`]={...doc.services[`edge_${id}`],volumes:[`${root}/${id}/pds-tls:/config:ro`],networks:{[`${id}_repository`]:{aliases:[`pds-tls-${id}.federation.example.com`]}}};
    doc.services[`pds_${id}`]={image:pdsImage,env_file:[`${root}/${id}/pds.env`],
      volumes:[`${id}_pds:/pds`],networks:[`${id}_repository`],security_opt:['no-new-privileges:true']};
    for (const [part,network] of [['gateway',`${id}_database`],['instance',`${id}_local`]]) {
      doc.services[`${part}_db_${id}`]={image:postgresImage,environment:{POSTGRES_USER:'fixture_admin',POSTGRES_DB:'memory_isolation',POSTGRES_PASSWORD_FILE:'/run/secrets/password'},
        volumes:[`${id}_${part}_db:/var/lib/postgresql/data`,`${root}/${id}/${part}-db-password:/run/secrets/password:ro`],networks:[network],security_opt:['no-new-privileges:true']};
    }
    // Replace only this process with an independently configured Codifix instance
    // for deployment. It gets its own DB and member token, never gateway secrets.
    doc.services[`member_${id}`]={image:pdsImage,...hardening,user:String(uid),entrypoint:['node','/probe/probe.cjs'],
      volumes:[`${root}/${id}/member:/member:ro`,`${root}/probe.cjs:/probe/probe.cjs:ro`],networks:['members',`${id}_local`]};
  }
  return doc;
}
export function tlsProxy(hostname,upstream,paths) {
  return `{\n admin off\n auto_https off\n servers {\n protocols h1 h2\n }\n}\nhttps://${hostname}:8443 {\n tls /config/cert.pem /config/key.pem\n @allowed path ${paths.join(' ')}\n route {\n reverse_proxy @allowed ${upstream}\n respond 404\n }\n}\n`;
}
