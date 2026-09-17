#!/usr/bin/env python3
"""Verify the native deck against approved generated content and untouched copy."""
import json,sys,pathlib,hashlib,re,copy
from PIL import Image
ROOT=pathlib.Path(__file__).parent

def unwrap(x):
    if isinstance(x,dict):
        if isinstance(x.get('slides'),list):return x
        for v in x.values():
            z=unwrap(v)
            if z:return z
    if isinstance(x,list):
        for v in x:
            z=unwrap(v)
            if z:return z
    if isinstance(x,str):
        try:return unwrap(json.loads(x))
        except ValueError:pass

def tx(e):return ''.join(t.get('textRun',{}).get('content','') for t in e.get('shape',{}).get('text',{}).get('textElements',[])).rstrip('\n')

def stable_background(props):
    props=copy.deepcopy(props)
    picture=props.get('pageBackgroundFill',{}).get('stretchedPictureFill',{})
    picture.pop('contentUrl',None) # provider-generated temporary delivery URL
    return props

def point(e):
    f=12700 if e['size']['width']['unit']=='EMU' else 1
    t=e['transform'];tf=12700 if t['unit']=='EMU' else 1
    x=t.get('translateX',0)/tf;y=t.get('translateY',0)/tf
    return [x,y,x+e['size']['width']['magnitude']/f*t.get('scaleX',0),y+e['size']['height']['magnitude']/f*t.get('scaleY',0)]

raw=pathlib.Path(sys.argv[1]);copyfile=pathlib.Path(sys.argv[2]);p=unwrap(json.loads(raw.read_text()));src=unwrap(json.loads(copyfile.read_text()))
content=json.loads((ROOT/'slide-content.json').read_text());manifest=json.loads((ROOT/'object-manifest.json').read_text())
errors=[];stats={'textFrames':0,'notes':0,'sourceLinks':0,'noteLinks':0,'logos':0,'tables':0,'connectorsAndRules':0,'nativeShapes':0}
def ck(cond,msg):
    if not cond:errors.append(msg)
ck([s['objectId'] for s in p['slides']]==[s['pageId'] for s in content],'Slide order mismatch')
ck(len(p['slides'])==10,'Expected10slides')
ck(p['pageSize']==src['pageSize'],'Canvas changed')
for slide,m in zip(content,manifest):
    page=next(s for s in p['slides'] if s['objectId']==slide['pageId']);original=next(s for s in src['slides'] if s['objectId']==slide['pageId'])
    elements={e['objectId']:e for e in page['pageElements']};old={e['objectId']:e for e in original['pageElements']}
    expected={e['id'] for e in m['elements'] if e['action']!='delete'}
    ck(set(elements)==expected,f"{slide['number']}: unexpected/missing native objects")
    ck(stable_background(page.get('pageProperties',{}))==stable_background(original.get('pageProperties',{})),f"{slide['number']}: background properties changed")
    for item in slide['items']:
        e=elements[item['id']];ck('shape'in e,f"{item['id']}: text not native");ck(tx(e)==item['text'],f"{item['id']}: text mismatch");stats['textFrames']+=1
        runs=[t['textRun'] for t in e['shape']['text']['textElements'] if 'textRun' in t and t['textRun']['content'].strip()]
        ck(all(t['style'].get('fontFamily')=='Arial' for t in runs),f"{item['id']}: wrong font")
        if item.get('link'):
            ck(all(t['style'].get('link',{}).get('url')==item['link'] for t in runs),f"{item['id']}: missing source link");stats['sourceLinks']+=1
    for item in m['elements']:
        if item['action']=='delete':continue
        e=elements[item['id']]
        if item['role']=='Codifix native logo':ck(e==old[item['id']],f"{item['id']}: logo changed");stats['logos']+=1
        if item['role'] in ['connector','rule']:
            ck('line'in e,f"{item['id']}: line not native");actual=point(e);ck(max(abs(a-b) for a,b in zip(actual,item['endpoints']))<0.06,f"{item['id']}: endpoints {actual} vs {item['endpoints']}");stats['connectorsAndRules']+=1
        if item['role']=='native financial table':
            ck('table'in e,'Financial table rasterized');stats['tables']+=1
            actual=[[ ''.join(t.get('textRun',{}).get('content','') for t in c.get('text',{}).get('textElements',[])).rstrip('\n') for c in row['tableCells']] for row in e['table']['tableRows']]
            ck(actual==item['values'],'Financial table transcription mismatch')
        if 'shape'in e:stats['nativeShapes']+=1
    nid=slide['notesId'];notes=next(e for e in page['slideProperties']['notesPage']['pageElements'] if e['objectId']==nid)
    ck(tx(notes)==slide['notes'],f"{slide['number']}: speaker note mismatch");stats['notes']+=1
    links={t.get('textRun',{}).get('style',{}).get('link',{}).get('url') for t in notes['shape']['text']['textElements']};links.discard(None)
    stats['noteLinks']+=len(links)
    urls={m.group(0).rstrip('.,;') for m in re.finditer(r'https://[^\s]+',slide['notes'])}
    ck(urls<=links,f"{slide['number']}: unlinked note URLs {urls-links}")
# The only background resource difference is a temporary contentUrl refresh.
# Verify rendered empty edge pixels as well as stable native properties.
work=raw.parent
background_errors=[];unchanged=[]
for slide in content:
    source_index=next(i+1 for i,page in enumerate(src['slides']) if page['objectId']==slide['pageId'])
    a=Image.open(work/'renders'/'source'/f'slide-{source_index:02}.png').convert('RGB')
    b=Image.open(work/'renders'/'final'/f"slide-{slide['number']:02}.png").convert('RGB')
    maximum=0
    for xx in [.01,.99]:
        for yy in [.02,.25,.50,.75,.98]:
            av=a.getpixel((round(xx*(a.width-1)),round(yy*(a.height-1))))
            bv=b.getpixel((round(xx*(b.width-1)),round(yy*(b.height-1))))
            maximum=max(maximum,max(abs(x-y) for x,y in zip(av,bv)))
    ck(maximum<=2,f"{slide['number']}: background edge pixels changed {maximum}")
    oldrender=work/'renders'/'v1'/f"slide-{slide['number']:02}.png"
    finalrender=work/'renders'/'final'/f"slide-{slide['number']:02}.png"
    if oldrender.read_bytes()==finalrender.read_bytes():unchanged.append(slide['number'])
visible='\n'.join(tx(e) for s in p['slides'] for e in s['pageElements'])
for banned in ['perfect recall','18.1%','41% fewer tokens','95% of pilots','140%']:
    ck(banned.lower() not in visible.lower(),f'Stale claim present: {banned}')
ck('Proposed integration using supported interfaces' in visible,'Databricks status absent')
ck('Neuralese · future research' in visible,'Neuralese status absent')
result={'verified':not errors,'revision':p['revisionId'],'presentationId':p['presentationId'],'sourceCopySha256':hashlib.sha256(copyfile.read_bytes()).hexdigest(),'nativeJsonSha256':hashlib.sha256(raw.read_bytes()).hexdigest(),'counts':stats,'unchangedInspectedRenders':unchanged,'backgroundCheck':'Stable native properties plus ten empty-edge pixel samples per slide; temporary provider contentUrl excluded','slideOrder':[s['objectId'] for s in p['slides']],'errors':errors}
(ROOT/'native-validation.json').write_text(json.dumps(result,indent=2));print(json.dumps(result,indent=2))
# Revision-specific acceptance: untouched slides and visible future designation.
extra=[]
for index in [0,1,2,3,4,5,6,8,9]:
 before=src['slides'][index];after=p['slides'][index]
 if before['pageElements']!=after['pageElements']:extra.append(f'Unexpected native content edit on slide {index+1}')
 if before['slideProperties']['notesPage']['pageElements']!=after['slideProperties']['notesPage']['pageElements']:extra.append(f'Unexpected note edit on slide {index+1}')
if 'Planned outreach' not in visible:extra.append('Future notification not visibly designated')
if 'Next: notify the responsible human.' not in visible:extra.append('Requested human notification roadmap label absent')
result['preservedSlides']=[1,2,3,4,5,6,7,9,10]
result['financialAppendixUnchanged']=not any('slide 10' in e for e in extra)

for phrase in ['BAE Systems client engagement','Shell engineering leadership','McKinsey technology consulting','Established relationships','Repeatable integrations','Measured pilots','Validated delivery economics','Discuss milestones and financing.']:
    if phrase not in visible:extra.append('Founder requirement missing: '+phrase)
result['errors'].extend(extra);result['verified']=not result['errors']
(ROOT/'native-validation.json').write_text(json.dumps(result,indent=2));print(json.dumps({'extraChecksPassed':not extra,'preservedSlides':result['preservedSlides'],'errors':extra}));sys.exit(bool(result['errors']))
