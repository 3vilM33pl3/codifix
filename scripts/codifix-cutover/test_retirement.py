# SPDX-License-Identifier: AGPL-3.0-or-later
"""Synthetic disposable PostgreSQL tests; never use a production URL here."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import unittest
import uuid

spec=importlib.util.spec_from_file_location('retire',Path(__file__).with_name('federation-retire.py'))
op=importlib.util.module_from_spec(spec);spec.loader.exec_module(op)

class ManifestTests(unittest.TestCase):
    def test_hash_excludes_own_hash_but_rejects_modified_review(self):
        payload={'text':'metadata','values':[1,2]}
        m={'payload':payload,'sha256':op.digest(payload)}
        self.assertEqual(op.manifest_hash(m),m['sha256'])
        m['sha256']='changed'
        self.assertEqual(op.manifest_hash(m),op.digest(payload))
        with self.assertRaises(ValueError):op.validate_manifest(m)
        with self.assertRaises(ValueError):op.canonical({'f':float('nan')})
        with self.assertRaises(ValueError):op.identifier(str(uuid.UUID(int=0)))

@unittest.skipUnless(os.environ.get('CODIFIX_CUTOVER_TEST_DATABASE_URL'),'explicit disposable test DB required')
class DatabaseTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import psycopg2
        from psycopg2 import sql
        cls.psycopg2=psycopg2;cls.sql=sql
        cls.admin=psycopg2.connect(os.environ['CODIFIX_CUTOVER_TEST_DATABASE_URL']);cls.admin.autocommit=True
        cls.name='codifix_retirement_'+uuid.uuid4().hex
        with cls.admin.cursor() as c:c.execute(sql.SQL('CREATE DATABASE {}').format(sql.Identifier(cls.name)))
        options=cls.admin.get_dsn_parameters();options['dbname']=cls.name
        cls.db=psycopg2.connect(**options)
        with cls.db.cursor() as c:
            c.execute('CREATE TABLE _sqlx_migrations(version BIGINT PRIMARY KEY,description TEXT NOT NULL,installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),success BOOLEAN NOT NULL,checksum BYTEA NOT NULL,execution_time BIGINT NOT NULL)')
            for p in sorted((op.ROOT/'migrations').glob('*.sql')):
                text=p.read_text()
                if int(p.name.split('_')[0])==4:
                    # Same dimensionless-HNSW compatibility as runtime/test-support;
                    # original applied SQLx checksum remains unchanged.
                    text=text.split('CREATE INDEX IF NOT EXISTS idx_memory_chunks_embedding_hnsw')[0]
                c.execute(text)
                c.execute('INSERT INTO _sqlx_migrations VALUES(%s,%s,now(),true,%s,0)',(int(p.name.split('_')[0]),p.stem,hashlib.sha384(p.read_bytes()).digest()))
        cls.db.commit()
    @classmethod
    def tearDownClass(cls):
        cls.db.close()
        with cls.admin.cursor() as c:c.execute(cls.sql.SQL('DROP DATABASE {} WITH (FORCE)').format(cls.sql.Identifier(cls.name)))
        cls.admin.close()
    def test_exact_retirement_preserves_statuses_payloads_origins_and_supports_guarded_rebind(self):
        from psycopg2.extras import Json
        local=str(uuid.uuid4());space=str(uuid.uuid4());project=str(uuid.uuid4());device=str(uuid.uuid4());memory=str(uuid.uuid4());company=str(uuid.uuid4())
        old={'scope':{'company_id':company,'area':'workshop','owner_instance_id':local,'project_id':project,'space_id':space},'device_id':device,'manifest_revision':1,'manifest_sha256':'a'*64}
        with self.db.cursor() as c:
            c.execute("INSERT INTO projects(id,slug,name,root_path) VALUES(%s,'synthetic','synthetic','/synthetic')",(project,))
            c.execute("INSERT INTO federation_gateway_spaces(space,authority,writer_epoch,projection) VALUES(%s,%s,%s,'{}')",(space,local,str(uuid.uuid4())))
            c.execute("INSERT INTO federation_replicas(space,receiver,authority,projection) VALUES(%s,%s,%s,'{}')",(space,local,local))
            c.execute("INSERT INTO federation_replica_trust(space,receiver,keys) VALUES(%s,%s,%s)",(space,local,Json([{'generation':1,'first_sequence':1,'public_key':'did:key:synthetic-only'}])))
            c.execute('INSERT INTO laptop_curated_bindings(local_project_id,binding,enabled) VALUES(%s,%s,true)',(project,Json(old)))
            c.execute("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES(%s,%s,%s,1,'preserved local fact','fact','decision','project',3,0.8,now(),now(),to_tsvector('simple','fact'))",(memory,project,memory))
            c.execute("UPDATE laptop_curated_outbox SET state='running',lease_token=%s,lease_until=now()+interval '1 day'",(str(uuid.uuid4()),))
            c.execute('INSERT INTO laptop_curated_origins VALUES(%s,%s,7)',(memory,str(uuid.uuid4())))
            tables,schema=op.schema(c)
            payload={'format':op.FORMAT,'migration_head':85,'schema_sha256':schema,'migrations':op.migrations(c),'database':op.database(c),'local_instance':local,'spaces':op.spaces(c,local),'before':op.fingerprints(c,tables),'preserved':op.fingerprints(c,tables,True)}
        self.db.commit();m={'payload':payload,'sha256':op.digest(payload)}
        with self.assertRaises(ValueError):op.apply(self.db,m,False)
        self.db.rollback()
        with self.db.cursor() as c:c.execute("UPDATE memory_entries SET summary='drift' WHERE id=%s",(memory,))
        with self.assertRaisesRegex(ValueError,'row drift'):op.apply(self.db,m,True)
        self.db.rollback()
        with self.db.cursor() as c:c.execute('CREATE TABLE unreviewed_dependency(id integer)')
        with self.assertRaisesRegex(ValueError,'drift'):op.apply(self.db,m,True)
        self.db.rollback()
        op.apply(self.db,m,True)
        op.apply(self.db,m,True) # exact retry verifies existing receipt/effects
        with self.db.cursor() as c:
            c.execute('SELECT state,binding,payload,lease_until=\'epoch\'::timestamptz FROM laptop_curated_outbox')
            status,binding,payload,expired=c.fetchone()
            self.assertEqual(status,'running');self.assertEqual(binding,old);self.assertIsNone(payload);self.assertTrue(expired)
            c.execute('SELECT owner_version FROM laptop_curated_origins');self.assertEqual(c.fetchone(),(7,))
            c.execute('SELECT count(*) FROM codifix_retired_federation_spaces');self.assertEqual(c.fetchone(),(1,))
        # Unknown retry manifests cannot rotate again or approve changed history.
        altered=json.loads(json.dumps(m));altered['payload']['spaces'][0]['writer_epoch_after']=str(uuid.uuid4());altered['sha256']=op.digest(altered['payload'])
        with self.assertRaisesRegex(ValueError,'receipt mismatch'):op.apply(self.db,altered,True)
        self.db.rollback()
        new=json.loads(json.dumps(old));new['scope']['space_id']=str(uuid.uuid4());new['manifest_revision']=2;new['manifest_sha256']='b'*64
        with self.db.cursor() as c:
            # Old identity cannot be replaced with an unrelated owner/project.
            wrong=json.loads(json.dumps(new));wrong['scope']['owner_instance_id']=str(uuid.uuid4())
            with self.assertRaises(self.psycopg2.Error):c.execute('SELECT codifix_rebind_laptop_project(%s,%s,%s,%s)',(project,Json(old),Json(wrong),'c'*64))
        self.db.rollback()
        for field in ['manifest_sha256','manifest_revision']:
            invalid=json.loads(json.dumps(new));invalid[field]=None
            with self.db.cursor() as c:
                with self.assertRaises(self.psycopg2.Error):c.execute('SELECT codifix_rebind_laptop_project(%s,%s,%s,%s)',(project,Json(old),Json(invalid),'c'*64))
            self.db.rollback()
        with self.db.cursor() as c:
            for _ in range(2):c.execute('SELECT codifix_rebind_laptop_project(%s,%s,%s,%s)',(project,Json(old),Json(new),'c'*64))
            c.execute('SELECT binding,enabled FROM laptop_curated_bindings');self.assertEqual(c.fetchone(),(new,True))
            c.execute('SELECT binding,state FROM laptop_curated_outbox');self.assertEqual(c.fetchone(),(old,'running'))
            c.execute('SELECT owner_version FROM laptop_curated_origins');self.assertEqual(c.fetchone(),(7,))
            c.execute('SELECT count(*) FROM codifix_laptop_binding_rebinds');self.assertEqual(c.fetchone(),(1,))
        self.db.commit()
        # Applied migration and checkpoint receipts are append-only.
        with self.db.cursor() as c:
            with self.assertRaises(self.psycopg2.Error):c.execute('DELETE FROM codifix_retired_federation_spaces')
        self.db.rollback()

if __name__=='__main__':unittest.main()
