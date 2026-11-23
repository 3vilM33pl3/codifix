#!/usr/bin/env python3
"""Codifix 3.0 one-time exact laptop binding rebind; never transfers old queues.

Inspect prints metadata only. Prepare a new enrolled binding JSON independently;
this operator never enrols, contacts an owner or alters canonical origin mappings.
All old processes must be stopped and drained before apply.
Fresh-device mode separately pins normal approval/subscription-preview evidence,
requires accepted receipted queues, and records one atomic transition using the
existing immutable receipt table. It never renews server authority or migrates
historical profiles, projections, origins or queued work.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import sys

spec=importlib.util.spec_from_file_location('retire',Path(__file__).with_name('federation-retire.py'))
retire=importlib.util.module_from_spec(spec);spec.loader.exec_module(retire)
FORMAT='codifix-3.0-laptop-rebind-v1'
FRESH_FORMAT='codifix-3.0-laptop-fresh-device-rebind-v1'

def binding(value):
    if set(value)!={'scope','device_id','manifest_revision','manifest_sha256'} or set(value['scope'])!={'company_id','area','owner_instance_id','project_id','space_id'}: raise ValueError('exact binding fields required')
    for key in ['company_id','owner_instance_id','project_id','space_id']:retire.identifier(value['scope'][key])
    retire.identifier(value['device_id'])
    if value['scope']['area'] not in ['general','workshop','homelab'] or not isinstance(value['manifest_revision'],int) or value['manifest_revision']<1 or not re.fullmatch('[0-9a-f]{64}',value['manifest_sha256']): raise ValueError('invalid new binding')

def fresh_identity(project, old, new):
    """The independently approved bootstrap changes only device/space/manifest."""
    retire.identifier(project);binding(old);binding(new)
    if (old['scope']['project_id'] != project or new['scope']['project_id'] != project
        or {k:v for k,v in old['scope'].items() if k!='space_id'} != {k:v for k,v in new['scope'].items() if k!='space_id'}
        or old['device_id']==new['device_id'] or old['scope']['space_id']==new['scope']['space_id']
        or type(old['manifest_revision']) is not int or type(new['manifest_revision']) is not int):
        raise ValueError('fresh device requires exact original scope and distinct device/space')

def evidence_pin(value):
    path=Path(value)
    if not path.is_absolute() or path.resolve(strict=True)!=path or not path.is_file() or path.stat().st_size>1024*1024:
        raise ValueError('bounded absolute fresh approval/bootstrap evidence files required')
    return {'path':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}

def fresh_evidence(payload):
    # These are independently retrieved normal-enrollment artifacts, not an
    # offline grant of authority. Pin both the reviewed approval evidence and
    # the authenticated CLI subscription preview that supplied this binding.
    evidence=payload['evidence']
    if set(evidence)!={'bootstrap','approval'}:raise ValueError('fresh approval/bootstrap evidence required')
    for item in evidence.values():
        if set(item)!={'path','sha256'} or evidence_pin(item['path'])!=item:
            raise ValueError('fresh approval/bootstrap evidence changed')
    preview=json.loads(Path(evidence['bootstrap']['path']).read_text());bootstrap=preview['bootstrap'];new=payload['new_binding']
    scope=bootstrap['scope'];expected={'company_id':scope['company'],'area':scope['area'],'owner_instance_id':scope['owner'],'project_id':scope['project'],'space_id':scope['space']}
    if (bootstrap['device']!=new['device_id'] or expected!=new['scope']
        or bootstrap['catalog_revision']!=new['manifest_revision'] or preview['manifest_sha256']!=new['manifest_sha256']):
        raise ValueError('fresh authenticated bootstrap differs from reviewed binding')

def drained_queue(cursor, project):
    # Only accepted, receipted work is terminal in the current contribution
    # state machine. Draft/blocked/uncertain work must never be abandoned.
    cursor.execute("SELECT EXISTS(SELECT FROM laptop_curated_outbox WHERE local_project_id=%s AND (state<>'accepted' OR receipt IS NULL OR lease_until>clock_timestamp()))",(project,))
    if cursor.fetchone()[0]:raise ValueError('all original contributions must be accepted, receipted and drained')
    cursor.execute("SELECT count(*),encode(sha256(convert_to(COALESCE(jsonb_agg(to_jsonb(t) ORDER BY operation_id),'[]'::jsonb)::text,'UTF8')),'hex') FROM laptop_curated_outbox t WHERE local_project_id=%s",(project,))
    count,digest=cursor.fetchone()
    return {'rows':count,'sha256':digest}

def fresh_receipt(cursor, project, old, new, retirement_sha256):
    fresh_identity(project,old,new)
    cursor.execute('SELECT manifest_sha256 FROM codifix_retired_federation_spaces WHERE space=%s',(old['scope']['space_id'],))
    if cursor.fetchone()!=(retirement_sha256,):raise ValueError('old retirement receipt mismatch')
    cursor.execute('SELECT EXISTS(SELECT FROM codifix_retired_federation_spaces WHERE space=%s)',(new['scope']['space_id'],))
    if cursor.fetchone()[0]:raise ValueError('fresh space is already retired')

def apply_fresh(cursor, payload, manifest_sha256):
    """One recorded fresh-device transition; caller owns the transaction."""
    from psycopg2.extras import Json
    project=payload['local_project'];old=payload['old_binding'];new=payload['new_binding']
    fresh_evidence(payload)
    cursor.execute("SET LOCAL lock_timeout='4000ms'")
    # Block both new queue inserts and lease/status changes while checking the
    # reviewed drain. Retirement inserts cannot race the fresh-space check.
    cursor.execute('LOCK TABLE laptop_curated_bindings,laptop_curated_outbox,codifix_laptop_binding_rebinds IN SHARE ROW EXCLUSIVE MODE')
    cursor.execute('LOCK TABLE codifix_retired_federation_spaces IN SHARE MODE')
    fresh_receipt(cursor,project,old,new,payload['retirement_sha256'])
    if drained_queue(cursor,project)!=payload['queue']:raise ValueError('reviewed contribution queue changed')
    cursor.execute('SELECT binding,enabled FROM laptop_curated_bindings WHERE local_project_id=%s FOR UPDATE',(project,));saved=cursor.fetchone()
    cursor.execute('SELECT old_binding,new_binding,manifest_sha256 FROM codifix_laptop_binding_rebinds WHERE local_project_id=%s',(project,));receipt=cursor.fetchone()
    if receipt:
        if receipt!=(old,new,manifest_sha256) or saved!=(new,True):raise ValueError('fresh device receipt mismatch')
        return
    if saved!=(old,False):raise ValueError('exact retired disabled old binding required')
    cursor.execute('INSERT INTO codifix_laptop_binding_rebinds(local_project_id,old_binding,new_binding,manifest_sha256) VALUES(%s,%s,%s,%s)',(project,Json(old),Json(new),manifest_sha256))
    cursor.execute('UPDATE laptop_curated_bindings SET binding=%s,enabled=TRUE WHERE local_project_id=%s AND binding=%s AND NOT enabled',(Json(new),project,Json(old)))
    if cursor.rowcount!=1:raise ValueError('original binding changed; no transition committed')
    fresh_evidence(payload)

def main():
    import psycopg2
    from psycopg2.extras import Json
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=['inspect','apply']);parser.add_argument('--project');parser.add_argument('--new-binding');parser.add_argument('--fresh-device',action='store_true',help='review an independently approved fresh-device bootstrap; never enrol or transfer old queues');parser.add_argument('--fresh-bootstrap',help='authenticated normal CLI subscription preview JSON');parser.add_argument('--fresh-approval',help='independently reviewed normal company approval evidence');parser.add_argument('--schema-reference',required=True);parser.add_argument('--manifest');parser.add_argument('--review-sha256');parser.add_argument('--writers-stopped',action='store_true');args=parser.parse_args()
    dsn=os.environ.get('CODIFIX_CUTOVER_DATABASE_URL')
    if not dsn:raise ValueError('explicit CODIFIX_CUTOVER_DATABASE_URL required')
    reference=json.loads(Path(args.schema_reference).read_text())
    with psycopg2.connect(dsn) as db:
        if args.command=='inspect':db.set_session(readonly=True,isolation_level='REPEATABLE READ')
        with db.cursor() as c:
            if reference!={'format':retire.FORMAT,'migrations':retire.migrations(c),'schema_sha256':retire.schema(c)[1]}:raise ValueError('candidate schema reference mismatch')
            if args.command=='inspect':
                project=retire.identifier(args.project);new=json.loads(Path(args.new_binding).read_text());binding(new)
                c.execute('SELECT binding,enabled FROM laptop_curated_bindings WHERE local_project_id=%s',(project,));saved=c.fetchone()
                if not saved or saved[1]:raise ValueError('retired disabled old binding required')
                old=saved[0];binding(old)
                c.execute('SELECT manifest_sha256 FROM codifix_retired_federation_spaces WHERE space=%s',(old['scope']['space_id'],));receipt=c.fetchone()
                if not receipt:raise ValueError('old space retirement receipt required')
                payload={'format':FORMAT,'database':retire.database(c),'local_project':project,'retirement_sha256':receipt[0],'old_binding':old,'new_binding':new}
                if args.fresh_device:
                    if not args.fresh_bootstrap or not args.fresh_approval:raise ValueError('fresh approval/bootstrap evidence required')
                    fresh_receipt(c,project,old,new,receipt[0])
                    c.execute('SELECT EXISTS(SELECT FROM codifix_laptop_binding_rebinds WHERE local_project_id=%s)',(project,))
                    if c.fetchone()[0]:raise ValueError('prior binding transition receipt exists')
                    payload.update(format=FRESH_FORMAT,queue=drained_queue(c,project),evidence={'bootstrap':evidence_pin(args.fresh_bootstrap),'approval':evidence_pin(args.fresh_approval)})
                    fresh_evidence(payload)
                print(json.dumps({'payload':payload,'sha256':retire.digest(payload)},indent=2));return
            m=json.loads(Path(args.manifest).read_text())
            if set(m)!={'payload','sha256'} or retire.manifest_hash(m)!=m['sha256'] or args.review_sha256!=m['sha256'] or not args.writers_stopped:raise ValueError('exact reviewed manifest and stopped/drained writers required')
            p=m['payload']
            if p.get('format')==FRESH_FORMAT:
                if not args.fresh_device or set(p)!={'format','database','local_project','retirement_sha256','old_binding','new_binding','queue','evidence'} or p['database']!=retire.database(c):raise ValueError('explicit fresh device manifest/target required')
                apply_fresh(c,p,m['sha256'])
                db.commit()
                print(json.dumps({'state':'rebound','manifest_sha256':m['sha256'],'mode':'fresh_device'}));return
            if args.fresh_device:raise ValueError('fresh device requires its separate reviewed manifest format')
            if set(p)!={'format','database','local_project','retirement_sha256','old_binding','new_binding'} or p['format']!=FORMAT or p['database']!=retire.database(c):raise ValueError('rebind manifest/target mismatch')
            retire.identifier(p['local_project']);binding(p['old_binding']);binding(p['new_binding'])
            c.execute('SELECT manifest_sha256 FROM codifix_retired_federation_spaces WHERE space=%s',(p['old_binding']['scope']['space_id'],))
            if c.fetchone()!=(p['retirement_sha256'],):raise ValueError('old retirement receipt mismatch')
            # SQL function holds the current binding row lock and permanently
            # binds one receipt to exact old/new identities, without touching origins.
            c.execute('SELECT codifix_rebind_laptop_project(%s,%s,%s,%s)',(p['local_project'],Json(p['old_binding']),Json(p['new_binding']),m['sha256']))
        db.commit()
        print(json.dumps({'state':'rebound','manifest_sha256':m['sha256']}))

if __name__=='__main__':
    try:main()
    except Exception as error:
        print('Codifix rebind rejected: '+(str(error) if isinstance(error,ValueError) else type(error).__name__),file=sys.stderr);sys.exit(1)
