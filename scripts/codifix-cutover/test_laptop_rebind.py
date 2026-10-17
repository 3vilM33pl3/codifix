# SPDX-License-Identifier: AGPL-3.0-or-later
"""Fresh-device operator coverage; DB URL must name a disposable admin fixture."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import uuid

HERE=Path(__file__).parent

def load(name, path):
    spec=importlib.util.spec_from_file_location(name,path)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    return module

op=load('laptop_rebind',HERE/'laptop-rebind.py')
fixtures=load('retirement_fixture',HERE/'test_retirement.py')

def identifier():return str(uuid.uuid4())

def pair():
    project=identifier()
    old={'scope':{'company_id':identifier(),'area':'workshop','owner_instance_id':identifier(),'project_id':project,'space_id':identifier()},'device_id':identifier(),'manifest_revision':1,'manifest_sha256':'a'*64}
    new=copy.deepcopy(old);new.update(device_id=identifier(),manifest_revision=2,manifest_sha256='b'*64);new['scope']['space_id']=identifier()
    return project,old,new

class IdentityTests(unittest.TestCase):
    def test_only_original_scope_with_distinct_device_and_space_is_allowed(self):
        project,old,new=pair();op.fresh_identity(project,old,new)
        for key in ['company_id','area','owner_instance_id','project_id']:
            bad=copy.deepcopy(new);bad['scope'][key]='homelab' if key=='area' else identifier()
            with self.subTest(key=key),self.assertRaises(ValueError):op.fresh_identity(project,old,bad)
        for key,value in [('device_id',old['device_id']),('manifest_revision',True),('device_id',str(uuid.UUID(int=0)))]:
            bad=copy.deepcopy(new);bad[key]=value
            with self.subTest(key=key,value=value),self.assertRaises(ValueError):op.fresh_identity(project,old,bad)
        bad=copy.deepcopy(new);bad['scope']['space_id']=old['scope']['space_id']
        with self.assertRaises(ValueError):op.fresh_identity(project,old,bad)

@unittest.skipUnless(os.environ.get('CODIFIX_CUTOVER_TEST_DATABASE_URL'),'explicit disposable test DB required')
class FreshDeviceDatabaseTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):fixtures.DatabaseTests.setUpClass.__func__(cls)
    @classmethod
    def tearDownClass(cls):fixtures.DatabaseTests.tearDownClass.__func__(cls)
    def setUp(self):
        from psycopg2.extras import Json
        self.project,self.old,self.new=pair();self.memory=identifier();self.operation=identifier()
        self.evidence=tempfile.TemporaryDirectory();self.addCleanup(self.evidence.cleanup)
        self.bootstrap=Path(self.evidence.name)/'bootstrap.json';self.approval=Path(self.evidence.name)/'approval.jsonl'
        scope=self.new['scope']
        self.bootstrap.write_text(json.dumps({'bootstrap':{'device':self.new['device_id'],'catalog_revision':self.new['manifest_revision'],'scope':{'company':scope['company_id'],'area':scope['area'],'owner':scope['owner_instance_id'],'project':scope['project_id'],'space':scope['space_id']}},'manifest_sha256':self.new['manifest_sha256']}))
        self.approval.write_text(json.dumps({'synthetic_approved_device':self.new['device_id']})+'\n')
        with self.db.cursor() as c:
            c.execute('INSERT INTO projects(id,slug,name,root_path) VALUES(%s,%s,%s,%s)',(self.project,'fixture-'+self.project,'fixture','/fixture/'+self.project))
            c.execute('INSERT INTO laptop_curated_bindings(local_project_id,binding,enabled) VALUES(%s,%s,false)',(self.project,Json(self.old)))
            c.execute("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES(%s,%s,%s,1,'original fact','original','decision','project',3,0.8,now(),now(),to_tsvector('simple','original'))",(self.memory,self.project,self.memory))
            c.execute("INSERT INTO laptop_curated_outbox(operation_id,local_project_id,local_memory_id,binding,state,payload,receipt) VALUES(%s,%s,%s,%s,'accepted','{\"original\":true}','{\"accepted\":true}')",(self.operation,self.project,self.memory,Json(self.old)))
            c.execute('INSERT INTO laptop_curated_origins VALUES(%s,%s,3)',(self.memory,identifier()))
            c.execute("INSERT INTO codifix_retired_federation_spaces(space,local_instance,manifest_sha256,replicas,manifest) VALUES(%s,%s,%s,'[]','{}')",(self.old['scope']['space_id'],self.old['scope']['owner_instance_id'],'c'*64))
            self.payload={'format':op.FRESH_FORMAT,'database':op.retire.database(c),'local_project':self.project,'retirement_sha256':'c'*64,'old_binding':self.old,'new_binding':self.new,'queue':op.drained_queue(c,self.project),'evidence':{'bootstrap':op.evidence_pin(self.bootstrap),'approval':op.evidence_pin(self.approval)}}
        self.sha=op.retire.digest(self.payload)
    def tearDown(self):self.db.rollback()
    def apply(self):
        with self.db.cursor() as c:op.apply_fresh(c,self.payload,self.sha)
    def test_exact_retry_preserves_every_other_table_and_schema(self):
        with self.db.cursor() as c:
            tables,schema=op.retire.schema(c);before=op.retire.fingerprints(c,[t for t in tables if t!='laptop_curated_bindings'])
        self.apply();self.apply()
        with self.db.cursor() as c:
            c.execute('SELECT binding,enabled FROM laptop_curated_bindings WHERE local_project_id=%s',(self.project,));self.assertEqual(c.fetchone(),(self.new,True))
            c.execute('SELECT old_binding,new_binding,manifest_sha256 FROM codifix_laptop_binding_rebinds WHERE local_project_id=%s',(self.project,));self.assertEqual(c.fetchone(),(self.old,self.new,self.sha))
            self.assertEqual(before,op.retire.fingerprints(c,[t for t in tables if t!='laptop_curated_bindings']))
            self.assertEqual(schema,op.retire.schema(c)[1])
    def test_nonterminal_work_and_missing_receipts_refuse(self):
        for state in ['queued','proposed','conflict','blocked']:
            with self.db.cursor() as c:
                c.execute('UPDATE laptop_curated_outbox SET state=%s WHERE operation_id=%s',(state,self.operation))
            with self.subTest(state=state),self.assertRaisesRegex(ValueError,'accepted'):self.apply()
        with self.db.cursor() as c:c.execute("UPDATE laptop_curated_outbox SET state='accepted',receipt=NULL WHERE operation_id=%s",(self.operation,))
        with self.assertRaisesRegex(ValueError,'receipted'):self.apply()
    def test_live_lease_refuses_even_when_work_is_accepted(self):
        with self.db.cursor() as c:c.execute("UPDATE laptop_curated_outbox SET lease_until=now()+interval '1 day' WHERE operation_id=%s",(self.operation,))
        with self.assertRaisesRegex(ValueError,'drained'):self.apply()
    def test_queue_drift_after_review_refuses(self):
        with self.db.cursor() as c:c.execute("UPDATE laptop_curated_outbox SET receipt='{\"changed\":true}' WHERE operation_id=%s",(self.operation,))
        with self.assertRaisesRegex(ValueError,'queue changed'):self.apply()
    def test_evidence_drift_and_unbound_fresh_device_refuse(self):
        self.approval.write_text('changed approval evidence\n')
        with self.assertRaisesRegex(ValueError,'evidence changed'):self.apply()
        self.payload['evidence']['approval']=op.evidence_pin(self.approval)
        self.payload['new_binding']=copy.deepcopy(self.new);self.payload['new_binding']['device_id']=identifier()
        with self.assertRaisesRegex(ValueError,'bootstrap differs'):self.apply()
    def test_enabled_old_binding_and_prior_conflicting_receipt_refuse(self):
        from psycopg2.extras import Json
        with self.db.cursor() as c:c.execute('UPDATE laptop_curated_bindings SET enabled=true WHERE local_project_id=%s',(self.project,))
        with self.assertRaisesRegex(ValueError,'disabled'):self.apply()
        with self.db.cursor() as c:
            c.execute('UPDATE laptop_curated_bindings SET enabled=false WHERE local_project_id=%s',(self.project,))
            c.execute('INSERT INTO codifix_laptop_binding_rebinds(local_project_id,old_binding,new_binding,manifest_sha256) VALUES(%s,%s,%s,%s)',(self.project,Json(self.old),Json(self.new),'d'*64))
        with self.assertRaisesRegex(ValueError,'receipt mismatch'):self.apply()
    def test_retry_never_reactivates_a_disabled_new_binding(self):
        self.apply()
        with self.db.cursor() as c:c.execute('UPDATE laptop_curated_bindings SET enabled=false WHERE local_project_id=%s',(self.project,))
        with self.assertRaisesRegex(ValueError,'receipt mismatch'):self.apply()
    def test_retirement_digest_and_already_retired_target_refuse(self):
        self.payload['retirement_sha256']='e'*64
        with self.assertRaisesRegex(ValueError,'retirement receipt'):self.apply()
        self.payload['retirement_sha256']='c'*64
        with self.db.cursor() as c:c.execute("INSERT INTO codifix_retired_federation_spaces(space,local_instance,manifest_sha256,replicas,manifest) VALUES(%s,%s,%s,'[]','{}')",(self.new['scope']['space_id'],self.new['scope']['owner_instance_id'],'f'*64))
        with self.assertRaisesRegex(ValueError,'already retired'):self.apply()
    def test_existing_same_device_sql_function_still_rejects_new_device(self):
        from psycopg2.extras import Json
        with self.db.cursor() as c:
            with self.assertRaises(self.psycopg2.Error):c.execute('SELECT codifix_rebind_laptop_project(%s,%s,%s,%s)',(self.project,Json(self.old),Json(self.new),self.sha))
    def test_failed_update_rolls_back_receipt_insert(self):
        with self.db.cursor() as c:
            c.execute("CREATE FUNCTION fail_fresh_binding_update() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected update failure'; END $$")
            c.execute('CREATE TRIGGER fail_fresh_binding_update BEFORE UPDATE ON laptop_curated_bindings FOR EACH ROW EXECUTE FUNCTION fail_fresh_binding_update()')
        self.db.commit()
        with self.assertRaises(self.psycopg2.Error):
            with self.db:self.apply()
        with self.db.cursor() as c:
            c.execute('SELECT count(*) FROM codifix_laptop_binding_rebinds WHERE local_project_id=%s',(self.project,));self.assertEqual(c.fetchone(),(0,))
            c.execute('SELECT binding,enabled FROM laptop_curated_bindings WHERE local_project_id=%s',(self.project,));self.assertEqual(c.fetchone(),(self.old,False))
            c.execute('DROP TRIGGER fail_fresh_binding_update ON laptop_curated_bindings');c.execute('DROP FUNCTION fail_fresh_binding_update()')
        self.db.commit()
    def test_real_cli_requires_review_and_stopped_writers_then_retries_exactly(self):
        with self.db.cursor() as c:reference={'format':op.retire.FORMAT,'migrations':op.retire.migrations(c),'schema_sha256':op.retire.schema(c)[1]}
        self.db.commit()
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'new.json').write_text(json.dumps(self.new));(root/'reference.json').write_text(json.dumps(reference))
            env={**os.environ,'CODIFIX_CUTOVER_DATABASE_URL':self.db.dsn}
            def run(*args):return subprocess.run([sys.executable,str(HERE/'laptop-rebind.py'),*args,'--schema-reference',str(root/'reference.json')],capture_output=True,text=True,env=env)
            preview=run('inspect','--fresh-device','--project',self.project,'--new-binding',str(root/'new.json'),'--fresh-bootstrap',str(self.bootstrap),'--fresh-approval',str(self.approval));self.assertEqual(preview.returncode,0,preview.stderr);manifest=json.loads(preview.stdout);self.assertEqual(manifest['payload']['format'],op.FRESH_FORMAT);(root/'manifest.json').write_text(preview.stdout)
            args=['apply','--manifest',str(root/'manifest.json'),'--review-sha256',manifest['sha256']]
            self.assertNotEqual(run(*args).returncode,0)
            self.assertNotEqual(run(*args,'--writers-stopped').returncode,0)
            for _ in range(2):
                result=run(*args,'--fresh-device','--writers-stopped');self.assertEqual(result.returncode,0,result.stderr)
            self.assertNotEqual(run('apply','--fresh-device','--manifest',str(root/'manifest.json'),'--review-sha256','0'*64,'--writers-stopped').returncode,0)

if __name__=='__main__':unittest.main()
