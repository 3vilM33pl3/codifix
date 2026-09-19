// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import test from 'node:test';
import { archiveFraming } from './archive-framing.mjs';
const start=(table,n)=>({event:'table_start',table,row_count:n,columns:[],foreign_keys:[]});
const end=(table,n)=>({event:'table_end',table,row_count:n});
const row=table=>({event:'original_row',record:{table,key:'synthetic'}});
const complete={event:'complete',table_count:2,row_count:1};
const events=()=>[start('public.a',0),end('public.a',0),start('public.b',1),row('public.b'),end('public.b',1)];
test('strict framing accounts for empty tables and original rows',()=>{
  const f=archiveFraming(['public.a','public.b'],1);
  assert.deepEqual(events().map(e=>f.consume(e)),[false,false,false,true,false]);f.complete(complete);
});
test('missing, duplicated, nested, unsorted and mismatched table boundaries fail closed',()=>{
  for(const change of [es=>es.splice(0,2),es=>es.splice(1,1),es=>es[0].row_count=1,es=>es[3].record.table='public.a',
    es=>es[4].row_count=2,es=>es.unshift(start('unsafe.other',0)),es=>es.push(start('public.a',0)),
    es=>es.splice(3,0,row('public.b')),es=>es[0].columns=null]){
    const f=archiveFraming(['public.a','public.b'],1),es=events();change(es);
    assert.throws(()=>{for(const e of es)f.consume(e);f.complete(complete);});
  }
  assert.throws(()=>archiveFraming(['public.a','public.b'],1).consume(row('public.a')));
});
