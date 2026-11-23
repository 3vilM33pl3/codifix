#!/usr/bin/env python3
"""Codifix 3.0 only: inspect/retire exact reviewed local federation state.

Requires psycopg2 and a schema reference from a pristine migrated candidate DB.
All old writers, schedulers, provider work and network calls must be stopped and
DRAINED before apply. No original row is deleted and no payload is rewritten.
Connection secrets are read only from CODIFIX_CUTOVER_DATABASE_URL.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import uuid

FORMAT = 'codifix-3.0-federation-retirement-v1'
ROOT = Path(__file__).resolve().parents[2]
MARKERS = {'codifix_retired_federation_spaces', 'codifix_laptop_binding_rebinds'}
EXCLUDE = {
    'federation_gateway_spaces': ['writer_epoch'],
    'federation_source_subscriptions': ['enabled'],
    'federation_source_outbox': ['blocked'],
    'federation_publish_outbox': ['blocked', 'lease_token', 'lease_until'],
    'federation_expertise_mailbox': ['lease_token', 'lease_until'],
    'federation_expertise_addressed': ['lease_token', 'lease_until', 'answer_lease_token', 'answer_lease_until', 'delete_lease_token', 'delete_lease_until'],
    'federation_expertise_deliveries': ['lease_token'],
    'laptop_curated_bindings': ['enabled'],
    'laptop_curated_outbox': ['lease_until'],
}
SPACE_TABLES = ('federation_gateway_spaces','federation_replicas','federation_source_subscriptions','federation_source_outbox')

def canonical(value):
    # This release manifest contains only metadata with ASCII keys, integers and
    # strings, never source text, floating-point numbers or private credentials.
    def check(v):
        if isinstance(v, float): raise ValueError('manifest floats are unsupported')
        if isinstance(v, dict):
            for k, x in v.items():
                if not isinstance(k, str) or not k.isascii(): raise ValueError('manifest keys')
                check(x)
        elif isinstance(v, list):
            for x in v: check(x)
    check(value)
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()

def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()

def identifier(value):
    parsed = uuid.UUID(value)
    if parsed.int == 0 or str(parsed) != value: raise ValueError('exact non-nil UUID required')
    return value

def manifest_hash(manifest):
    # Own hash is intentionally outside the canonical payload.
    return digest(manifest['payload'])

def validate_manifest(manifest):
    if set(manifest) != {'payload','sha256'} or manifest_hash(manifest) != manifest['sha256']:
        raise ValueError('review manifest hash mismatch')
    p = manifest['payload']
    if set(p) != {'format','migration_head','schema_sha256','migrations','database','local_instance','spaces','before','preserved'} or p['format'] != FORMAT or p['migration_head'] != 85:
        raise ValueError('unsupported release manifest')
    identifier(p['local_instance'])
    if not p['spaces'] or len({s['space'] for s in p['spaces']}) != len(p['spaces']): raise ValueError('exact spaces required')
    for s in p['spaces']:
        identifier(s['space'])
        if set(s) != {'space','writer_epoch_after','replicas'}: raise ValueError('space fields')
        if s['writer_epoch_after'] is not None: identifier(s['writer_epoch_after'])
        for r in s['replicas']:
            if set(r) != {'receiver','authority','cursor','history_sha256','keys'}: raise ValueError('replica fields')
            identifier(r['receiver']); identifier(r['authority'])
            if r['receiver'] != p['local_instance'] or not isinstance(r['cursor'],int) or not 0 <= r['cursor'] <= 4294967295 or not re.fullmatch('[0-9a-f]{64}',r['history_sha256']): raise ValueError('replica checkpoint')
            keys=r['keys']
            if not isinstance(keys,list) or not 1<=len(keys)<=32: raise ValueError('pinned key schedule')
            for i,k in enumerate(keys):
                if set(k)!={'generation','first_sequence','public_key'} or k['generation']!=i+1 or not isinstance(k['first_sequence'],int) or k['first_sequence']<1 or not isinstance(k['public_key'],str) or not 1<=len(k['public_key'])<=256: raise ValueError('pinned key schedule')
                if (i==0 and k['first_sequence']!=1) or (i and k['first_sequence']<=keys[i-1]['first_sequence']): raise ValueError('pinned key schedule')
            if len({k['public_key'] for k in keys})!=len(keys): raise ValueError('duplicate receipt key')
    return p

def migrations(cursor):
    cursor.execute('SELECT version,encode(checksum,\'hex\'),success FROM _sqlx_migrations ORDER BY version')
    actual = {str(v): checksum for v, checksum, success in cursor.fetchall() if success}
    expected = {str(int(p.name.split('_')[0])): hashlib.sha384(p.read_bytes()).hexdigest() for p in sorted((ROOT/'migrations').glob('*.sql'))}
    if max(map(int, expected)) != 85 or actual != expected: raise ValueError('upgrade to exact candidate schema85 first')
    return expected

def schema(cursor):
    cursor.execute("SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename")
    tables = [r[0] for r in cursor.fetchall()]
    if not MARKERS.issubset(tables) or not set(EXCLUDE).issubset(tables): raise ValueError('candidate tables missing')
    # Omit catalog OIDs and physical storage; include all user columns,
    # constraints, indexes, triggers and non-extension functions.
    cursor.execute("""SELECT jsonb_build_object(
      'columns',(SELECT jsonb_agg(to_jsonb(c) ORDER BY table_name,ordinal_position) FROM information_schema.columns c WHERE table_schema='public'),
      'constraints',(SELECT jsonb_agg(jsonb_build_array(c.relname,p.conname,pg_get_constraintdef(p.oid)) ORDER BY c.relname,p.conname) FROM pg_constraint p JOIN pg_class c ON c.oid=p.conrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public'),
      'indexes',(SELECT jsonb_agg(jsonb_build_array(tablename,indexname,indexdef) ORDER BY tablename,indexname) FROM pg_indexes WHERE schemaname='public'),
      'triggers',(SELECT jsonb_agg(pg_get_triggerdef(t.oid) ORDER BY c.relname,t.tgname) FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND NOT t.tgisinternal),
      'functions',(SELECT jsonb_agg(pg_get_functiondef(p.oid) ORDER BY p.proname,pg_get_function_identity_arguments(p.oid)) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public' AND p.prokind='f' AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.objid=p.oid AND d.deptype='e'))
    )""")
    raw = cursor.fetchone()[0]
    # information_schema's catalog field varies per DB, remove that identity.
    for c in raw['columns'] or []:
        for key in list(c):
            # A dump/restore compacts dropped physical column slots. These two
            # identifiers are storage history, not logical schema. Keep all
            # types, defaults, nullability and other semantic attributes exact.
            if key.endswith('_catalog') or key in ('ordinal_position','dtd_identifier'): c.pop(key)
    raw['columns'] = sorted(raw['columns'] or [], key=lambda c: (c['table_name'],c['column_name']))
    return tables, digest(raw)

def fingerprints(cursor, tables, preserve=False):
    from psycopg2 import sql
    result = {}
    for table in tables:
        if table in MARKERS: continue
        expression = sql.SQL('to_jsonb(t) - %s::text[]')
        ignored = EXCLUDE.get(table,[]) if preserve else []
        cursor.execute(sql.SQL("SELECT count(*),encode(sha256(convert_to(COALESCE(jsonb_agg(v ORDER BY v::text),'[]'::jsonb)::text,'UTF8')),'hex') FROM (SELECT ")+expression+sql.SQL(' AS v FROM public.{} t) rows').format(sql.Identifier(table)), (ignored,))
        count, checksum = cursor.fetchone()
        result[table] = {'rows':count,'sha256':checksum}
    return result

def database(cursor):
    cursor.execute("SELECT current_database(),(SELECT oid::text FROM pg_database WHERE datname=current_database()),COALESCE(inet_server_addr()::text,'local-socket'),inet_server_port()")
    return list(cursor.fetchone())

def spaces(cursor, local_instance):
    queries = [f'SELECT space FROM {t}' for t in SPACE_TABLES]
    queries += ["SELECT (scope->>'space')::uuid FROM device_replica_projections", "SELECT (scope->>'space')::uuid FROM onboarding_project_projections", "SELECT (binding->'scope'->>'space_id')::uuid FROM laptop_curated_bindings"]
    # ProjectScope uses company/owner/project/space; contribution bindings use *_id.
    cursor.execute(' UNION '.join(queries)+' ORDER BY 1')
    ids = [str(r[0]) for r in cursor.fetchall()]
    result=[]
    for space in ids:
        cursor.execute('SELECT authority FROM federation_gateway_spaces WHERE space=%s',(space,))
        gateway=cursor.fetchone()
        if gateway and str(gateway[0])!=local_instance: raise ValueError('gateway instance binding mismatch')
        cursor.execute('SELECT receiver,authority,cursor FROM federation_replicas WHERE space=%s ORDER BY receiver',(space,))
        replicas=[]
        for receiver,authority,seq in cursor.fetchall():
            if str(receiver)!=local_instance: raise ValueError('replica receiver binding mismatch')
            cursor.execute('SELECT keys FROM federation_replica_trust WHERE space=%s AND receiver=%s',(space,str(receiver)))
            keys=cursor.fetchone()
            if not keys: raise ValueError('retired replica requires pinned receipt trust')
            cursor.execute('SELECT codifix_retained_history_sha256(%s,%s)',(space,str(receiver)))
            replicas.append({'receiver':str(receiver),'authority':str(authority),'cursor':seq,'history_sha256':cursor.fetchone()[0],'keys':keys[0]})
        result.append({'space':space,'writer_epoch_after':str(uuid.uuid4()) if gateway else None,'replicas':replicas})
    return result

def retire(cursor, manifest):
    from psycopg2.extras import Json
    p=manifest['payload'];ids=[s['space'] for s in p['spaces']]
    for s in p['spaces']:
        cursor.execute('INSERT INTO codifix_retired_federation_spaces(space,local_instance,manifest_sha256,replicas,manifest) VALUES(%s,%s,%s,%s,%s)',(s['space'],p['local_instance'],manifest['sha256'],Json(s['replicas']),Json(manifest)))
        if s['writer_epoch_after']:
            cursor.execute('UPDATE federation_gateway_spaces SET writer_epoch=%s WHERE space=%s',(s['writer_epoch_after'],s['space']))
    cursor.execute('UPDATE federation_source_subscriptions SET enabled=FALSE WHERE space=ANY(%s::uuid[])',(ids,))
    cursor.execute('UPDATE federation_source_outbox SET blocked=TRUE WHERE space=ANY(%s::uuid[]) AND acceptance IS NULL',(ids,))
    cursor.execute('UPDATE federation_publish_outbox SET blocked=TRUE,lease_token=NULL,lease_until=NULL WHERE space=ANY(%s::uuid[]) AND NOT published',(ids,))
    cursor.execute('UPDATE federation_expertise_mailbox SET lease_token=NULL,lease_until=NULL WHERE space=ANY(%s::uuid[])',(ids,))
    cursor.execute('UPDATE federation_expertise_addressed SET lease_token=NULL,lease_until=NULL,answer_lease_token=NULL,answer_lease_until=NULL,delete_lease_token=NULL,delete_lease_until=NULL WHERE space=ANY(%s::uuid[])',(ids,))
    cursor.execute('UPDATE federation_expertise_deliveries SET lease_token=NULL WHERE space=ANY(%s::uuid[])',(ids,))
    cursor.execute("UPDATE laptop_curated_bindings SET enabled=FALSE WHERE (binding->'scope'->>'space_id')::uuid=ANY(%s::uuid[])",(ids,))
    # Keep running/accepted/proposed/conflict status truthful. The old binding is
    # disabled and marker blocks claim; an expired lease invalidates old ack.
    cursor.execute("UPDATE laptop_curated_outbox SET lease_until='epoch' WHERE (binding->'scope'->>'space_id')::uuid=ANY(%s::uuid[]) AND state='running'",(ids,))

def verify_effects(cursor, manifest):
    from psycopg2.extras import Json
    p=manifest['payload'];ids=[s['space'] for s in p['spaces']]
    for s in p['spaces']:
        cursor.execute('SELECT local_instance::text,manifest_sha256,replicas,manifest FROM codifix_retired_federation_spaces WHERE space=%s',(s['space'],))
        if cursor.fetchone() != (p['local_instance'],manifest['sha256'],s['replicas'],manifest): raise ValueError('retirement receipt mismatch')
        if s['writer_epoch_after']:
            cursor.execute('SELECT writer_epoch::text FROM federation_gateway_spaces WHERE space=%s',(s['space'],))
            if cursor.fetchone() != (s['writer_epoch_after'],): raise ValueError('writer fence changed')
    predicates=[
      ('federation_source_subscriptions','enabled'),('federation_source_outbox','acceptance IS NULL AND NOT blocked'),
      ('federation_publish_outbox','NOT published AND (NOT blocked OR lease_token IS NOT NULL OR lease_until IS NOT NULL)'),
      ('federation_expertise_mailbox','lease_token IS NOT NULL OR lease_until IS NOT NULL'),
      ('federation_expertise_addressed','lease_token IS NOT NULL OR answer_lease_token IS NOT NULL OR delete_lease_token IS NOT NULL'),
      ('federation_expertise_deliveries','lease_token IS NOT NULL')]
    for table,predicate in predicates:
        cursor.execute(f'SELECT EXISTS(SELECT 1 FROM {table} WHERE space=ANY(%s::uuid[]) AND ({predicate}))',(ids,))
        if cursor.fetchone()[0]: raise ValueError('retired execution effect drift')
    cursor.execute("SELECT EXISTS(SELECT 1 FROM laptop_curated_bindings WHERE (binding->'scope'->>'space_id')::uuid=ANY(%s::uuid[]) AND enabled)",(ids,))
    if cursor.fetchone()[0]: raise ValueError('retired laptop binding enabled')
    cursor.execute("SELECT EXISTS(SELECT 1 FROM laptop_curated_outbox WHERE (binding->'scope'->>'space_id')::uuid=ANY(%s::uuid[]) AND state='running' AND lease_until IS DISTINCT FROM 'epoch'::timestamptz)",(ids,))
    if cursor.fetchone()[0]: raise ValueError('retired laptop lease active')

def apply(connection, manifest, writers_stopped):
    from psycopg2 import sql
    if not writers_stopped: raise ValueError('stop and drain all old processes before apply')
    p=validate_manifest(manifest)
    with connection.cursor() as c:
        tables,checksum=schema(c)
        if migrations(c)!=p['migrations'] or checksum!=p['schema_sha256'] or database(c)!=p['database']: raise ValueError('candidate/schema/database drift')
        c.execute(sql.SQL('LOCK TABLE {} IN SHARE ROW EXCLUSIVE MODE').format(sql.SQL(',').join(sql.Identifier('public',t) for t in tables)))
        # Recheck schema after the lock, including unknown tables/dependencies.
        if schema(c)[1]!=checksum: raise ValueError('schema changed during lock')
        c.execute('SELECT space::text FROM codifix_retired_federation_spaces ORDER BY space')
        existing=[r[0] for r in c.fetchall()]
        intended=sorted(s['space'] for s in p['spaces'])
        if existing:
            if existing!=intended: raise ValueError('unknown/partial retirement markers')
            verify_effects(c,manifest)
        else:
            if fingerprints(c,tables)!=p['before']: raise ValueError('reviewed row drift')
            # Rebuild current IDs/checkpoints, ignore only pre-reviewed new epochs.
            current=spaces(c,p['local_instance'])
            if [{**s,'writer_epoch_after':s['writer_epoch_after'] is not None} for s in current]!=[{**s,'writer_epoch_after':s['writer_epoch_after'] is not None} for s in p['spaces']]: raise ValueError('space/checkpoint drift')
            retire(c,manifest)
            verify_effects(c,manifest)
        if fingerprints(c,tables,True)!=p['preserved']: raise ValueError('preserved rows changed')
    connection.commit()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=['schema-reference','inspect','apply'])
    parser.add_argument('--local-instance')
    parser.add_argument('--schema-reference')
    parser.add_argument('--manifest')
    parser.add_argument('--review-sha256')
    parser.add_argument('--writers-stopped',action='store_true')
    args=parser.parse_args()
    import psycopg2
    dsn=os.environ.get('CODIFIX_CUTOVER_DATABASE_URL')
    if not dsn: raise ValueError('explicit CODIFIX_CUTOVER_DATABASE_URL required')
    # No credential or DB response body is logged on failure.
    with psycopg2.connect(dsn) as connection:
        if args.command=='apply':
            m=json.loads(Path(args.manifest).read_text())
            if args.review_sha256!=m.get('sha256'): raise ValueError('exact reviewed hash required')
            apply(connection,m,args.writers_stopped)
            print(json.dumps({'state':'retired','manifest_sha256':m['sha256']}));return
        connection.set_session(readonly=True,isolation_level='REPEATABLE READ')
        with connection.cursor() as c:
            versions=migrations(c);tables,checksum=schema(c)
            if args.command=='schema-reference':
                print(json.dumps({'format':FORMAT,'migrations':versions,'schema_sha256':checksum},indent=2));return
            reference=json.loads(Path(args.schema_reference).read_text())
            if reference!={'format':FORMAT,'migrations':versions,'schema_sha256':checksum}: raise ValueError('pristine candidate schema reference mismatch')
            local=identifier(args.local_instance)
            c.execute('SELECT EXISTS(SELECT 1 FROM codifix_retired_federation_spaces)')
            if c.fetchone()[0]: raise ValueError('use original reviewed manifest for exact retirement retry')
            payload={'format':FORMAT,'migration_head':85,'schema_sha256':checksum,'migrations':versions,'database':database(c),'local_instance':local,'spaces':spaces(c,local),'before':fingerprints(c,tables),'preserved':fingerprints(c,tables,True)}
            result={'payload':payload,'sha256':digest(payload)}
            validate_manifest(result)
            print(json.dumps(result,indent=2))

if __name__=='__main__':
    try: main()
    except Exception as error:
        # DB exceptions may contain statements/rows/connection secrets.
        print('Codifix retirement rejected: '+(str(error) if isinstance(error,ValueError) else type(error).__name__),file=sys.stderr)
        sys.exit(1)
