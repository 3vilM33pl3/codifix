#!/usr/bin/env python3
"""Audit the native M4 readback against its content and retained design objects."""
import hashlib,json,pathlib,re,sys
HERE=pathlib.Path(__file__).parent
raw,source,renders,previous=map(pathlib.Path,sys.argv[1:5])
def find(o):
 if isinstance(o,dict):
  if isinstance(o.get('slides'),list):return o
  for v in o.values():
   x=find(v)
   if x:return x
 if isinstance(o,list):
  for v in o:
   x=find(v)
   if x:return x
 if isinstance(o,str):
  try:return find(json.loads(o))
  except ValueError:pass
def text(o):return ''.join(e.get('textRun',{}).get('content','') for e in o.get('textElements',[])).strip()
def norm(s):return re.sub(r'\s+',' ',s).strip()
p=find(json.loads(raw.read_text()));old=find(json.loads(source.read_text()));expected=json.loads((HERE/'slide-content.json').read_text());manifest=json.loads((HERE/'object-manifest.json').read_text());report=[]
assert len(p['slides'])==18
assert [s['objectId'] for s in p['slides']]==[s['pageId'] for s in expected]
assert p['pageSize']['width']['magnitude']/12700==960 and p['pageSize']['height']['magnitude']/12700==540
for i,(s,w,m) in enumerate(zip(p['slides'],expected,manifest),1):
 es={e['objectId']:e for e in s['pageElements']};source_slide=old['slides'][i-1 if i<=14 else 0]
 for e in m['elements']:
  if e['action']=='delete':assert e['id'] not in es
  elif e['role']=='native table':
   t=es[e['id']]['table'];actual=[[text(c.get('text',{})) for c in row['tableCells']] for row in t['tableRows']]
   assert actual==e['values'],(i,actual)
  elif 'text'in e:assert norm(text(es[e['id']]['shape']['text']))==norm(e['text']),(i,e['id'])
 for e in source_slide['pageElements'][-6:]:
  e=dict(e)
  if i>14:
   e['objectId']=e['objectId'].replace('p1_',f'm4_a{i}_')
   e['transform']=dict(e['transform'])
   # Native duplicateObject rounds scale values to four decimals (<0.012 pt).
   for k in ('scaleX','scaleY'):e['transform'][k]=round(e['transform'][k],4)
  assert es[e['objectId']]==e,(i,'logo changed')
 notes_page=s['slideProperties']['notesPage'];nid=notes_page['notesProperties']['speakerNotesObjectId']
 note=next(e for e in notes_page['pageElements'] if e['objectId']==nid)
 assert norm(text(note['shape']['text']))==norm(w['notes']),(i,'notes mismatch')
 links=[t['textRun']['style']['link']['url'] for e in es.values() for t in e.get('shape',{}).get('text',{}).get('textElements',[]) if t.get('textRun',{}).get('style',{}).get('link',{}).get('url')]
 assert links and not any('/blob/89104dd/'in l for l in links)
 assert not any('image'in e for e in es.values()),(i,'flattened media')
 title=next(e for e in m['elements'] if e.get('role')=='title');assert title['fontSize']==30
 if i not in range(2,7):assert (renders/f'slide-{i:02}.png').read_bytes()==(previous/f'slide-{i:02}.png').read_bytes(),(i,'unexpected visual change')
 report.append({'slide':i,'pageId':s['objectId'],'visibleContent':'exact match','notes':'exact match','logoObjectsPreserved':6,'sourceLinks':len(links),'nativeTables':sum('table'in e for e in es.values())})
# Lines are normalized by Google to 3,000,000 EMU, so verify actual endpoints.
endpoints={2:[(416,270,539,270)],3:[(311,273,343,273),(617,273,649,273)],4:[(389,272,571,272),(571,295,389,295)],5:[(311,211,341,211),(615,211,645,211),(781,285,781,297),(645,375,615,375),(341,375,311,375)],6:[(48,217,910,217),(48,316,910,316)]}
for i,coords in endpoints.items():
 es=[e for e in p['slides'][i-1]['pageElements'] if '_edge_'in e['objectId']]
 assert len(es)==len(coords)
 for e,target in zip(es,coords):
  t=e['transform'];u=12700 if t['unit']=='EMU' else 1;w=e['size']['width']['magnitude']/12700;h=e['size']['height']['magnitude']/12700
  actual=[t['translateX']/u,t['translateY']/u,t['translateX']/u+w*t.get('scaleX',0),t['translateY']/u+h*t.get('scaleY',0)]
  assert max(abs(a-b) for a,b in zip(actual,target))<.03,(i,actual,target)
result={'verifiedDate':'2026-09-17','revisionId':p['revisionId'],'presentationId':p['presentationId'],'slideCount':18,'canvasPt':[960,540],'rawSha256':hashlib.sha256(raw.read_bytes()).hexdigest(),'pdfSha256':hashlib.sha256((renders/'presentation.pdf').read_bytes()).hexdigest(),'nativeEditability':'Text, semantic diagrams and three native tables; zero flattened slide images','connectorEndpoints':'12 correct within 0.03 pt','unchangedRenderPages':'1 and 7–18 byte-identical to the already inspected previous pass','slides':report}
(HERE/'native-validation.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k!='slides'},indent=2))
