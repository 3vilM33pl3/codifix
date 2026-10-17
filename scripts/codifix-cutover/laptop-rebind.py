#!/usr/bin/env python3
"""Codifix 3.0 one-time exact laptop binding rebind; never transfers old queues.

Inspect prints metadata only. Prepare a new enrolled binding JSON independently;
this operator never enrols, contacts an owner or alters canonical origin mappings.
All old processes must be stopped and drained before apply.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import sys

spec=importlib.util.spec_from_file_location('retire',Path(__file__).with_name('federation-retire.py'))
retire=importlib.util.module_from_spec(spec);spec.loader.exec_module(retire)
FORMAT='codifix-3.0-laptop-rebind-v1'

def binding(value):
    if set(value)!={'scope','device_id','manifest_revision','manifest_sha256'} or set(value['scope'])!={'company_id','area','owner_instance_id','project_id','space_id'}: raise ValueError('exact binding fields required')
    for key in ['company_id','owner_instance_id','project_id','space_id']:retire.identifier(value['scope'][key])
    retire.identifier(value['device_id'])
    if value['scope']['area'] not in ['general','workshop','homelab'] or not isinstance(value['manifest_revision'],int) or value['manifest_revision']<1 or not re.fullmatch('[0-9a-f]{64}',value['manifest_sha256']): raise ValueError('invalid new binding')

def main():
    import psycopg2
    from psycopg2.extras import Json
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=['inspect','apply']);parser.add_argument('--project');parser.add_argument('--new-binding');parser.add_argument('--schema-reference',required=True);parser.add_argument('--manifest');parser.add_argument('--review-sha256');parser.add_argument('--writers-stopped',action='store_true');args=parser.parse_args()
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
                print(json.dumps({'payload':payload,'sha256':retire.digest(payload)},indent=2));return
            m=json.loads(Path(args.manifest).read_text())
            if set(m)!={'payload','sha256'} or retire.manifest_hash(m)!=m['sha256'] or args.review_sha256!=m['sha256'] or not args.writers_stopped:raise ValueError('exact reviewed manifest and stopped/drained writers required')
            p=m['payload']
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
