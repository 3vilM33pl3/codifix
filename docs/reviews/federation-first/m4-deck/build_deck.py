#!/usr/bin/env python3
"""Generate native Slides requests from the retained M4 source; no API calls.

Inputs are immutable source JSON plus freshly read native speaker-note IDs.
Reuses source title/text/card objects, table and logos. Writes request batches,
slide content, notes and an explicit keep/reuse/delete manifest for review.
"""
import copy,json,pathlib,sys,re
HERE=pathlib.Path(__file__).parent
SOURCE=pathlib.Path(sys.argv[1])
NOTES=json.loads(pathlib.Path(sys.argv[2]).read_text())
OUT=pathlib.Path(sys.argv[3]);OUT.mkdir(parents=True,exist_ok=True)
def unwrap(o):
 if isinstance(o,dict):
  if isinstance(o.get('slides'),list):return o
  for v in o.values():
   x=unwrap(v)
   if x:return x
 if isinstance(o,list):
  for v in o:
   x=unwrap(v)
   if x:return x
 if isinstance(o,str):
  try:return unwrap(json.loads(o))
  except ValueError:pass
p=unwrap(json.loads(SOURCE.read_text()))
INK='1A1A1A'; MUTED='61565D'; PINK='F7D9E6';BLUE='DEE4F2';PEACH='F7E3D9';WHITE='FFFFFF'
def rgb(h):return {k:int(h[i:i+2],16)/255 for k,i in [('red',0),('green',2),('blue',4)]}
def tx(e):return ''.join(t.get('textRun',{}).get('content','') for t in e.get('shape',{}).get('text',{}).get('textElements',[]))
slides=[];manifest=[]
class Slide:
 def __init__(self,n,title):
  self.n=n;self.orig=copy.deepcopy(p['slides'][n-1 if n<=14 else 0]);self.pid=self.orig['objectId'] if n<=14 else f'm4_appendix_{n}'
  if n>14:
   for e in self.orig['pageElements']:e['objectId']=e['objectId'].replace('p1_',f'm4_a{n}_')
  self.elements=self.orig['pageElements'];self.used=set(e['objectId'] for e in self.elements[-6:]);self.req=[];self.serial=0;self.words=[];self.map=[]
  self.text(title,48,37,864,76,30,True,id=self.elements[0]['objectId'],role='title')
 def new(self,kind):self.serial+=1;return f'm4_s{self.n}_{kind}_{self.serial}'
 def get(self,mode,id=None):
  if id:
   e=next(x for x in self.elements if x['objectId']==id)
  else:
   e=next((x for x in self.elements[:-6] if x['objectId'] not in self.used and 'shape'in x and ((mode=='text' and tx(x).strip()) or (mode=='box' and not tx(x).strip() and x['shape']['shapeType'] in ['RECTANGLE','ROUND_RECTANGLE']))),None)
  if e:self.used.add(e['objectId'])
  return e
 def pos(self,id,x,y,w,h,e=None):
  if e:
   unit=e['size']['width']['unit'];factor=12700 if unit=='EMU' else 1
   sx=w/(e['size']['width'].get('magnitude',0)/factor);sy=h/(e['size']['height'].get('magnitude',0)/factor)
   if 'table' in e:sx=sy=1
   self.req.append({'updatePageElementTransform':{'objectId':id,'applyMode':'ABSOLUTE','transform':{'scaleX':sx,'scaleY':sy,'translateX':x,'translateY':y,'unit':'PT'}}})
 def text(self,text,x,y,w,h,size=20,bold=False,color=INK,align='START',id=None,role='body',link=None):
  e=self.get('text',id);id=e['objectId'] if e else self.new('text')
  if e:
   self.req.append({'deleteText':{'objectId':id,'textRange':{'type':'ALL'}}});self.pos(id,x,y,w,h,e)
  else:
   self.req.append({'createShape':{'objectId':id,'shapeType':'TEXT_BOX','elementProperties':{'pageObjectId':self.pid,'size':{'width':{'magnitude':w,'unit':'PT'},'height':{'magnitude':h,'unit':'PT'}},'transform':{'scaleX':1,'scaleY':1,'translateX':x,'translateY':y,'unit':'PT'}}}})
  self.req.extend([{'insertText':{'objectId':id,'text':text,'insertionIndex':0}}, {'updateShapeProperties':{'objectId':id,'shapeProperties':{'shapeBackgroundFill':{'propertyState':'NOT_RENDERED'},'outline':{'propertyState':'NOT_RENDERED'},'contentAlignment':'TOP','autofit':{'autofitType':'NONE'}},'fields':'shapeBackgroundFill,outline,contentAlignment,autofit.autofitType'}}])
  style={'fontFamily':'Arial','fontSize':{'magnitude':size,'unit':'PT'},'bold':bold,'italic':False,'underline':False,'foregroundColor':{'opaqueColor':{'rgbColor':rgb(color)}}}
  if link:style['link']={'url':link}
  self.req.append({'updateTextStyle':{'objectId':id,'textRange':{'type':'ALL'},'style':style,'fields':'fontFamily,fontSize,bold,italic,underline,foregroundColor,link'}})
  self.req.append({'updateParagraphStyle':{'objectId':id,'textRange':{'type':'ALL'},'style':{'alignment':align,'lineSpacing':108,'spaceAbove':{'magnitude':0,'unit':'PT'},'spaceBelow':{'magnitude':0,'unit':'PT'},'indentStart':{'magnitude':0,'unit':'PT'},'indentEnd':{'magnitude':0,'unit':'PT'},'indentFirstLine':{'magnitude':0,'unit':'PT'}},'fields':'alignment,lineSpacing,spaceAbove,spaceBelow,indentStart,indentEnd,indentFirstLine'}})
  self.words.append(text);self.map.append({'id':id,'action':'reuse' if e else 'create','role':role,'text':text,'bounds':[x,y,w,h],'fontSize':size});return id
 def box(self,x,y,w,h,color=BLUE):
  e=self.get('box');id=e['objectId'] if e else self.new('box')
  if e:self.pos(id,x,y,w,h,e)
  else:self.req.append({'createShape':{'objectId':id,'shapeType':'ROUND_RECTANGLE','elementProperties':{'pageObjectId':self.pid,'size':{'width':{'magnitude':w,'unit':'PT'},'height':{'magnitude':h,'unit':'PT'}},'transform':{'scaleX':1,'scaleY':1,'translateX':x,'translateY':y,'unit':'PT'}}}})
  self.req.extend([{'updateShapeProperties':{'objectId':id,'shapeProperties':{'shapeBackgroundFill':{'solidFill':{'color':{'rgbColor':rgb(color)},'alpha':1}},'outline':{'propertyState':'NOT_RENDERED'}},'fields':'shapeBackgroundFill,outline'}},{'updatePageElementsZOrder':{'pageElementObjectIds':[id],'operation':'SEND_TO_BACK'}}])
  self.map.append({'id':id,'action':'reuse' if e else 'create','role':'semantic panel','bounds':[x,y,w,h]});return id
 def line(self,x,y,x2,y2,arrow=False,color=MUTED):
  id=self.new('edge');self.req.append({'createLine':{'objectId':id,'lineCategory':'STRAIGHT','elementProperties':{'pageObjectId':self.pid,'size':{'width':{'magnitude':max(abs(x2-x),1),'unit':'PT'},'height':{'magnitude':max(abs(y2-y),1),'unit':'PT'}},'transform':{'scaleX':0 if x2==x else 1 if x2>x else -1,'scaleY':0 if y2==y else 1 if y2>y else -1,'translateX':x,'translateY':y,'unit':'PT'}}}})
  self.req.append({'updateLineProperties':{'objectId':id,'lineProperties':{'lineFill':{'solidFill':{'color':{'rgbColor':rgb(color)},'alpha':1}},'weight':{'magnitude':1.5,'unit':'PT'},'dashStyle':'SOLID','endArrow':'FILL_ARROW' if arrow else 'NONE'},'fields':'lineFill,weight,dashStyle,endArrow'}})
 def table(self,values,x,y,w,h,widths=None,existing=None,font=18):
  r=len(values);c=len(values[0]);id=existing or self.new('table')
  if existing:
   self.used.add(id)
   e=next(e for e in self.elements if e['objectId']==id)
   assert e['table']['rows']==r and e['table']['columns']==c
   self.pos(id,x,y,w,h,e)
   for i in range(r):
    for j in range(c):self.req.append({'deleteText':{'objectId':id,'cellLocation':{'rowIndex':i,'columnIndex':j},'textRange':{'type':'ALL'}}})
  else:self.req.append({'createTable':{'objectId':id,'rows':r,'columns':c,'elementProperties':{'pageObjectId':self.pid,'size':{'width':{'magnitude':w,'unit':'PT'},'height':{'magnitude':h,'unit':'PT'}},'transform':{'scaleX':1,'scaleY':1,'translateX':x,'translateY':y,'unit':'PT'}}}})
  if widths:
   for j,cw in enumerate(widths):self.req.append({'updateTableColumnProperties':{'objectId':id,'columnIndices':[j],'tableColumnProperties':{'columnWidth':{'magnitude':cw,'unit':'PT'}},'fields':'columnWidth'}})
  self.req.append({'updateTableRowProperties':{'objectId':id,'rowIndices':list(range(r)),'tableRowProperties':{'minRowHeight':{'magnitude':h/r,'unit':'PT'}},'fields':'minRowHeight'}})
  for i,row in enumerate(values):
   for j,t in enumerate(row):
    loc={'rowIndex':i,'columnIndex':j};ran={'location':loc,'rowSpan':1,'columnSpan':1}
    self.req.extend([{'insertText':{'objectId':id,'cellLocation':loc,'text':t,'insertionIndex':0}},{'updateTextStyle':{'objectId':id,'cellLocation':loc,'textRange':{'type':'ALL'},'style':{'fontFamily':'Arial','fontSize':{'magnitude':font,'unit':'PT'},'bold':i==0,'foregroundColor':{'opaqueColor':{'rgbColor':rgb(INK)}}},'fields':'fontFamily,fontSize,bold,foregroundColor'}},{'updateParagraphStyle':{'objectId':id,'cellLocation':loc,'textRange':{'type':'ALL'},'style':{'lineSpacing':106,'spaceAbove':{'magnitude':0,'unit':'PT'},'spaceBelow':{'magnitude':0,'unit':'PT'}},'fields':'lineSpacing,spaceAbove,spaceBelow'}},{'updateTableCellProperties':{'objectId':id,'tableRange':ran,'tableCellProperties':{'tableCellBackgroundFill':{'solidFill':{'color':{'rgbColor':rgb(BLUE if i==0 else WHITE)},'alpha':1}},'contentAlignment':'MIDDLE'},'fields':'tableCellBackgroundFill,contentAlignment'}}])
  self.map.append({'id':id,'action':'reuse' if existing else 'create','role':'native table','values':values,'bounds':[x,y,w,h]});self.words.extend(' | '.join(row) for row in values)
 def finish(self,status,source,notes):
  self.text(status,48,470,864,29,14,False,MUTED,role='evidence label')
  source_url=source[1]
  if source_url.startswith(BASE):
   source_url=('https://app.notion.com/p/3dc8990715f4815a812bf2a9262ef282' if '/m1-' in source_url else 'https://app.notion.com/p/3dc8990715f4813fb281d4a8175c3d1a' if '/m2-' in source_url else 'https://app.notion.com/p/3de8990715f481448e6dc392c3f853c4')
  notes=re.sub(re.escape(BASE)+r'[^\s]+',lambda m:'Repository evidence at 89104dd: '+m.group(0).removeprefix(BASE),notes)
  notes+='\nDated evidence dossier: https://app.notion.com/p/3de8990715f481448e6dc392c3f853c4'
  self.text(source[0],48,507,720,20,10,False,MUTED,role='source link',link=source_url)
  self.text(f'{self.n:02}',780,506,32,21,10,False,MUTED,role='slide number')
  noteid=NOTES[self.pid]
  self.req.append({'insertText':{'objectId':noteid,'text':notes,'insertionIndex':0}})
  for e in self.elements:
   if e['objectId'] not in self.used:
    self.req.append({'deleteObject':{'objectId':e['objectId']}});self.map.append({'id':e['objectId'],'action':'delete','reason':'Superseded claim, decoration or layout not used in approved narrative','old_text':tx(e)})
  for e in self.elements[-6:]:self.map.append({'id':e['objectId'],'action':'keep','role':'Codifix native logo'})
  slides.append({'number':self.n,'pageId':self.pid,'text':self.words,'notes':notes});manifest.append({'number':self.n,'pageId':self.pid,'elements':self.map})
  (OUT/f'slide-{self.n:02}.json').write_text(json.dumps(self.req,ensure_ascii=False,indent=2))
BASE='https://github.com/3vilM33pl3/memory/blob/89104dd/'
M1=BASE+'docs/reviews/federation-first/m1-execution.md'
ROAD=BASE+'docs/reviews/federation-first/README.md'
F1=BASE+'docs/reviews/federation-first/f1-execution.md'
ADR=BASE+'docs/developer/adr/0011-atproto-federation-adapter.md'
R3=BASE+'evals/r3/development-continuation-20260917.md'
LON='https://docs.google.com/spreadsheets/d/1PIUTGbZ2a-7sXsxAl6wbChJVTeKN8Q2K2rv1LRcBbO0/edit'
NY='https://docs.google.com/spreadsheets/d/1fT9Ta-IjX65NqwcRXFRJq5tW814aqje2Vha9YFfx2es/edit'
MODEL=BASE+'docs/reviews/federation-first/m2-legal-assurance-revision.md'

s=Slide(1,'Codifix')
s.text('Knowledge and governance\nfor the Agentic Enterprise',48,144,850,130,36,True)
s.text('Help teams find trusted expertise across boundaries\nand reuse what they learn — with local control.',48,320,836,95,22)
s.finish('Starting with incident handover between application and platform teams.',('Commercial foundation · M1',M1),f'Codifix is positioned as the knowledge and governance network for the Agentic Enterprise. The first commercial proof is incident handover between independently administered application and platform/operations teams. Customer value remains a hypothesis; no customer traction is implied.\nSource: {M1}\nPrepared 17 September 2026. Reusable investor deck; no fixed fundraising ask. The Tom briefing is pre-meeting research, not feedback, endorsement or investment interest.')

s=Slide(2,'The incident moves. The context does not.')
s.text('Illustrative incident · payment latency after a deployment',48,115,864,42,20,False,MUTED)
for x,c,title,body in [(48,BLUE,'Application team','Knows the change and symptoms.\nHands over a ticket and partial logs.'),(554,PEACH,'Platform team','Knows the infrastructure history.\nRebuilds the missing context.')]:
 s.box(x,189,358,166,c);s.text(title,x+18,207,324,39,22,True);s.text(body,x+18,256,324,90,20)
s.line(416,270,539,270,True)
s.text('handover',417,226,123,32,18,False,MUTED,'CENTER')
s.text('Repeated investigation',48,398,300,49,20,True)
s.text('Interrupted experts',352,398,260,49,20,True)
s.text('Corrections stay local',654,398,285,49,20,True)
s.finish('Pain and outcomes to validate in discovery; this is not a customer case study.',('Pilot preparation · M1',M1),f'This payment-latency incident is fictional and illustrates a cross-unit handover problem. No incident timings, savings or customer outcomes are claimed. Discovery must establish frequency, operational cost, missing context and access to a sponsor.\nSource: {M1}')

s=Slide(3,'From agentic work to an Agentic Enterprise')
for x,c,top,title,body in [(48,BLUE,'01','Agentic\nEngineering','Agents help people\nbuild software.'),(354,PEACH,'02','Agentic OS','An environment\nfor agentic work.\nOmarchy is an analogy.'),(660,PINK,'03','Agentic\nEnterprise','Business units exchange\nselected expertise\nunder local authority.')]:
 s.box(x,149,252,276,c);s.text(top,x+16,164,200,31,18,True,MUTED);s.text(title,x+16,209,220,78,23,True);s.text(body,x+16,309,220,104,19)
s.line(311,273,343,273,True);s.line(617,273,649,273,True)
s.finish('Broader vision: a network of business units. Current scope: governed knowledge exchange.',('Omarchy reference · no affiliation or dependency', 'https://omarchy.org/'),f'Agentic Engineering → Agentic OS → Agentic Enterprise is our narrative analogy, not a formal technical maturity model. Omarchy describes itself as agentic Linux. No affiliation or runtime dependency is implied. The current Memory implementation supports governed knowledge exchange and private Q&A; broader cross-unit delegated execution remains future work.\nSources: https://omarchy.org/\n{ADR}\n{ROAD}')

s=Slide(4,'Every unit owns its knowledge and authority')
for x,c,title in [(48,BLUE,'Application business unit'),(586,PEACH,'Platform business unit')]:
 s.box(x,145,326,270,c);s.text(title,x+17,162,292,65,22,True)
 s.text('Knowledge collections\nLocal agents\nIndependent administration',x+17,259,292,121,20)
s.text('Authorized\nconnection',384,173,193,70,19,True,INK,'CENTER')
s.line(389,272,571,272,True);s.line(571,295,389,295,True)
s.text('Selected expertise\n+ cited answers',384,322,193,80,18,False,INK,'CENTER')
s.finish('ATProto provides identity and signed records; Memory governs access and private Q&A.',('Architecture decision · AT Protocol foundation',ADR),f'Each vertex is an independently governed business unit with its own knowledge, administrators and agents. The edge represents explicitly authorized collaboration, not unrestricted data replication or agent execution. The illustration has two units; additional divisions and partners are expansion hypotheses. AT Protocol supplies identity, Lexicon schemas and signed repositories. Memory supplies authorization, publication history, recipient and version controls, and private Q&A through its gateway. Repository signatures establish authenticity and integrity within the verified scope; they do not prove factual truth, original human authorship or human review.\nSources: {ADR}\nhttps://atproto.com/guides/overview\n{F1}')

s=Slide(5,'One handover becomes reusable expertise')
steps=[('1  Handover','Responder identifies\nthe incident context.'),('2  Discover','Find permitted expertise\nacross the connection.'),('3  Cited answer','Source team returns an answer\nwith source citations.'),('6  Reuse later','A later eligible incident\nretrieves the correction.'),('5  Review at origin','The owning team approves\nthe corrected knowledge.'),('4  Human action','Responder checks, acts,\nand records corrections.')]
for i,(title,body) in enumerate(steps):
 x=48+(i%3)*304;y=143+(i//3)*164
 s.box(x,y,254,134,[BLUE,PEACH,PINK][i%3]);s.text(title,x+12,y+12,233,35,20,True);s.text(body,x+12,y+53,232,76,18)
s.line(311,211,341,211,True);s.line(615,211,645,211,True);s.line(781,285,781,297,True)
s.line(645,375,615,375,True);s.line(341,375,311,375,True)
s.finish('Components implemented; end-to-end customer improvement still requires a measured pilot.',('Federation / private Q&A evidence · 11 September 2026',F1),f'The sequence is the bounded pilot workflow: handover → permitted expertise discovery → cited answer → human action → origin-reviewed correction → later reuse. Current components include reviewed publication/import, provenance and private gateway Q&A. Human action, correctness and approved-correction reuse must be observed in the customer pilot; this diagram does not claim a measured full customer workflow. Correction reuse requires local curation and renewed collection review; count that effort. Changes to approved collection versions require review at the origin. Failed/refused exchanges remain in the denominator.\nSources: {F1}\n{M1}')

s=Slide(6,'Measure better handovers, not activity volume')
rows=[('Faster correct action','Handover → first correct action\nand accepted resolution.'),('Fewer expert interruptions','Interruptions and expert minutes\nper eligible incident.'),('Learning that survives','Approved-correction reuse\nand recurrence of corrected errors.')]
for i,(title,body) in enumerate(rows):
 y=135+i*99;s.text(title,48,y,339,74,21,True);s.text(body,430,y,476,74,20)
 if i<2:s.line(48,y+82,910,y+82)
s.box(48,439,865,30,PEACH);s.text('Recovered capacity becomes cash savings only when spending actually changes.',58,441,842,26,17,False)
s.finish('Agree thresholds and comparable cases before declaring success. Retain failures and refusals.',('Measurement definitions, denominators and owners · M1',M1),f'Compare against current handover documents/search with comparable information and resources. Match severity, service, novelty and responder familiarity. Every metric needs a definition, denominator, evidence source, owner and customer-agreed threshold. Missing thresholds or insufficient comparable cases prevent success declaration. Include correctness, useful completion, disclosure, refusals/failures, onboarding and infrastructure/inference/curation/admin/support costs. Recovered staff capacity is not automatically a cash saving. Avoid counting expert interruption savings twice.\nSource: {M1}')

s=Slide(7,'Implemented software; customer proof still ahead')
s.table([['Capability','Evidence today','Remaining proof'],['Governed federation','Signed, reviewed\nknowledge exchange','Customer deployment\nand recovery gates'],['Private gateway Q&A','Bounded request/answer\nand source citations','Answer quality and\noperational impact'],['Origin-owned corrections','Versioned publication\nand re-review controls','Measured correction\nreuse in later incidents']],48,133,864,253,[235,314,315],existing='p7_i8',font=18)
s.text('Controlled demonstrations used synthetic data and isolated services on one host.',48,407,865,58,19)
s.finish('Implementation is not validated quality, customer readiness or a production deployment.',('F1 completion evidence · 11 September 2026; R3 development · 17 September',F1),f'F1 covers two-way answers, restart behavior, third-instance consent and private TLS gateway/PDS isolation. It uses independent databases and real HTTP service applications/workers in one test process; the accompanying E gate separately starts service executables. Both are controlled synthetic evidence on one host, with fake provider synthesis in F1, not customer SSO or multi-host deployment. C supplies transactional federation outboxes/inboxes and receiver cursors; R1 supplies durable local jobs, leases, fencing and operator-visible failures. Broad recovery/release gates remain. As of 17 September, four-area work includes a materializer, atomic inactive-target importer and pre-dispatch input binding, but no activated or released four-area demonstration. Do not imply implementation alone establishes quality improvement.\nSources: {F1}\n{ROAD}\n{R3}\n{BASE}docs/reviews/four-area-network/README.md')

s=Slide(8,'Land with two units. Expand through trusted links.')
s.text('Economic buyer',48,132,356,34,18,True,MUTED);s.text('CIO / CTO or technology-\noperations leadership',48,181,378,83,25,True)
s.text('Operational champions',48,306,370,34,18,True,MUTED);s.text('Application and platform leaders\nwith named owners on both sides',48,353,389,77,20)
for i,(title,body,c) in enumerate([('Start','Two independently administered units',BLUE),('Expand internally','Divisions and acquired entities',PEACH),('Extend the network','Authorized external partners',PINK)]):
 y=127+i*104;s.box(479,y,432,87,c);s.text(title,495,y+8,400,29,20,True);s.text(body,495,y+42,400,37,18)
s.finish('Buyer and expansion logic are hypotheses to test; no customer or partner commitments claimed.',('Buyer definition and qualification · M1',M1),f'Initial economic buyers are CIO/CTO or technology-operations leadership. Operational champions own application/platform workflows and approvals. Qualification requires independently administered instances, owners on both sides, an approved sharing boundary and observable baseline, plus commercial and technical gates. Expansion through divisions, acquired entities and partners follows authorized edges; it is a strategy, not a commitment or network effect already measured.\nSource: {M1}')

s=Slide(9,'Compete on controlled exchange and ownership')
for i,(title,body) in enumerate([('Enterprise search & workplace agents','Glean and Rovo already offer connectors,\npermission-aware access and answers.'),('Agent memory platforms','Mem0, Zep and Letta offer persistent memory;\nshared memory and governance are established capabilities.')]):
 y=136+i*117;s.text(title,48,y,864,36,22,True);s.text(body,48,y+45,862,65,20)
s.box(48,383,865,76,BLUE);s.text('Codifix focus: exchange across independent administrations,\nwith corrections reviewed and owned by the originating team.',62,393,833,67,21,True)
s.finish('Defensibility must be earned through workflow adoption, governed history and delivery experience.',('Official competitor documentation · sources and scope in notes','https://www.glean.com/platform/api'),'''Competitor documentation reviewed 17 September 2026. Acknowledging adjacent capabilities is essential: Glean has permission-aware APIs and verified/deprecated content; Rovo honors source permissions and offers memory management; Mem0 markets persistent memory and governance; Zep documents RBAC/ABAC and audit logging; Letta documents shared memory and organization permissions. This review does not establish complete feature equivalence or absence of Codifix's focus elsewhere. Defensibility through adopted workflows, reviewed correction history and deployment expertise remains a hypothesis, not proof of moat.\nSources:\nhttps://www.glean.com/platform/api\nhttps://docs.glean.com/get-started/golive/populate-content\nhttps://www.atlassian.com/software/rovo/guides/admin-guide/rovo-connectors\nhttps://support.atlassian.com/rovo/docs/what-is-rovo-memory-management/\nhttps://mem0.ai/\nhttps://help.getzep.com/governance\nhttps://help.getzep.com/audit-logging\nhttps://docs.letta.com/concepts/shared-memory\nhttps://docs.letta.com/teams/permissions''')

s=Slide(10,'An enterprise platform commitment')
s.box(48,144,376,272,BLUE);s.text('£80,000 / year',64,175,343,56,32,True);s.text('One enterprise account\nTwo independently\nadministered deployments',64,258,343,112,21)
s.text('Separate commercial components',475,143,437,63,23,True);s.text('Additional deployments\nOnboarding\nUsage overages',475,236,428,117,22)
s.text('Replicated copies do not become\nseparately billable new knowledge.',475,379,432,75,20,True)
s.finish('Pricing hypothesis. Initial Base includes no pilot, onboarding or overage receipts.',('M2 Base commercial assumptions',LON),f'£80,000 annual platform package is a founder-selected, unvalidated price anchor for one enterprise account with two independently administered deployments. Other components remain separately adjustable hypotheses. Accounts, deployments, usage and replicated storage are tracked separately. Undefined legacy operations are not customer questions or provider tokens. Initial Base includes no pilot, onboarding or overage receipts. Commercial economics match across locations at planning FX $1.3476/£, dated 14 September 2026.\nSources: London Assumptions!E12:E27 (pricing, deployments, receipts and funnel), E11 (FX) {LON}\nNew York {NY}\n{MODEL}\nM3 validates costs and packaging; M5 provides customer evidence.')

s=Slide(11,'Founder-led discovery, then a qualified pilot')
s.text('Qualify the buyer and boundary',48,132,864,42,23,True)
s.text('Recurring pain · sponsor and budget access · independent control\nNamed owners · approved sharing · observable baseline',48,184,862,81,20)
for i,(num,title,body,c) in enumerate([('2 weeks','Baseline','Current handover workflow',BLUE),('4 weeks','Assisted operation','Comparable eligible incidents',PEACH),('2 weeks','Analysis','Quality, value and full cost',PINK)]):
 x=48+i*304;s.box(x,294,254,146,c);s.text(num,x+12-(7.2 if i==2 else 0),306-(3.6 if i==2 else 0),230,32,20,True);s.text(title,x+12-(7.2 if i==2 else 0),346-(7.2 if i==2 else 0),230,37,22,True);s.text(body,x+12-(7.2 if i==2 else 0),389-(7.2 if i==2 else 0),230,44,18)
s.finish('Commercial qualification cannot override deployment, security, recovery or evaluation gates.',('M1 scorecard and eight-week pilot charter',M1),f'Use the six-dimension 0–2 scorecard: recurring pain, measurable operational cost, independent control, sponsor/budget access, measurement feasibility and expansion potential. 9/12 is a provisional internal discovery-priority hypothesis, not a proven cutoff. Pilot qualification requires two independently administered instances, named owners, approved sharing boundaries and an observable baseline. The illustrative schedule is two baseline weeks, four assisted and two analysis weeks. Match comparable cases and retain failures/refusals. The eight-week illustration begins after deployment readiness and charter approval; inadequate case counts do not become success when the calendar ends. No pilot starts are customer commitments.\nSource: {M1}')

s=Slide(12,'Advance when the evidence supports it')
items=[('1','Deployment readiness','Security, authorization, recovery and evaluation gates.'),('2','Measured pilot outcome','Correctness, handover time, interruptions and correction reuse.'),('3','Validated delivery economics','Observed inference, support, curation and assurance costs.'),('4','Repeatable onboarding','Independent teams onboard within measured effort and limits.')]
for i,(n,title,body) in enumerate(items):
 y=133+i*81;s.text(n,48,y,46,49,28,True,MUTED);s.text(title,113,y,784,35,22,True);s.text(body,113,y+38,791,41,18)
s.finish('Readiness shifts the pipeline. There is no unconditional GA date or committed customer count.',('Evidence gates and commercial workstream handoffs',ROAD),f'Milestones are evidence-based rather than unconditional dates: deployment/security/recovery and evaluation readiness; customer-agreed pilot outcomes; actual delivery economics; repeatable onboarding. M2 models pilot starts in month five conditional on readiness, but delays shift the pipeline. M3 cost and packaging validation and M5 customer evidence remain open. Native Spaces, broader delegated execution and a production four-area supervisor are future work.\nSources: {ROAD}\n{M1}\n{MODEL}')

s=Slide(13,'Founder-led, with explicit assurance ownership')
s.text('Olivier Van Acker',48,136,352,42,25,True);s.text('Founder · creator of Memory Layer',48,188,374,55,19)
s.text('Built the Rust memory platform,\ncode-graph retrieval, MCP\nintegration and evaluation tooling.',48,268,375,117,20)
s.text('Planned organization',471,133,438,35,22,True)
s.text('Nine employees, including founder,\n+ fractional counsel\nEngineering, deployment, commercial,\nsupport and an assurance lead.',471,184,439,111,20)
s.text('Assurance lead owns the programme:\nISO 27001 · ISO 42001 · SOC 2 Type II',471,316,439,78,19,True)
s.text('Employee AI per person, per month:\n£5k allowance · £2.5k expected spend.',48,413,856,54,20,True)
s.finish('Hiring, AI utilization and throughput are hypotheses. Certifications are not completed.',('Current M2 staffing and employee AI schedules',MODEL),f'Founder shipped work is supported by the repository: Rust memory platform, code-graph retrieval, MCP integration and evaluation tooling. No unsupported release count, employer logo, candidate pipeline or cofounder commitment is included. Planned organization is nine employees at steady state (8/9/9 year-end headcount) plus fractional legal counsel, not nine employees already hired. The assurance lead owns the ISO 27001 and ISO 42001 certification programme and SOC 2 Type II attestation programme. Certification/attestation has not been completed; external audit/advisory costs are unquoted. SOC 2 Type II is an attestation, not an ISO certification. Employee AI budget is £5,000/month allowance at 50% utilization: £250 subscriptions plus £2,250 metered spending, with the other £2,500 retained as cash headroom. Assumptions!E119:E120 defines the allowance/utilization; Employee AI!C9:C12 defines per-employee spending/headroom; Team!B46 and AM46:AQ46 defines assurance leadership. These are operating hypotheses, not proven productivity or equivalent output from fewer people. M3 must measure AI spending, throughput, review/rework and delivery capacity.\nSources: {BASE}README.md\n{MODEL}\n{LON}\n{NY}')

s=Slide(14,'Fund the next proof points')
s.text('Capital should demonstrate',48,128,864,49,29,True)
for i,(title,body,c) in enumerate([('A trustworthy deployment','Pass security, recovery and evaluation gates.',BLUE),('A valuable workflow','Measure quality and operational outcomes in a qualified pilot.',PEACH),('A repeatable business','Validate delivery cost, packaging and onboarding effort.',PINK)]):
 y=201+i*77;s.box(48,y,865,67,c);s.text(title,61,y+10,312,49,21,True);s.text(body,391,y+10,507,49,19)
s.text('Discuss milestones and financing structure.',48,445,864,31,22,True)
s.finish('Round size follows the evidence, scope and runway required. No fixed fundraising ask.',('Provisional financial scenarios in the appendix',LON),f'This deck makes no fixed monetary fundraising ask. Discuss the proof points, sequencing, resources, financing structure and operating location first. The M2 London and New York figures in the appendix are alternative planning scenarios under a 24-month funding objective, not an approved round, spending plan or guarantee. M3 must validate costs, staffing capacity and assurance quotes; M5 supplies customer evidence. The Tom briefing anticipates investor questions but the conversation has not happened, so it supplies no feedback, endorsement or investment interest.\nSources: {MODEL}\n{ROAD}\n{LON}\n{NY}')

s=Slide(15,'Appendix · architecture and trust boundaries')
for x,c,title,body in [(48,BLUE,'AT Protocol foundation','Stable identity (DIDs)\nLexicon record schemas\nSigned repositories and sync'),(490,PEACH,'Memory governance','Authorization and publication history\nReviewed, versioned knowledge\nPrivate Q&A through the gateway')]:
 s.box(x,133,422,175,c);s.text(title,x+13,146,396,36,21,True);s.text(body,x+13,197,396,99,18)
s.text('Trust and disclosure limits',48,330,861,35,22,True)
s.text('Gateway availability and enforcement are trusted dependencies.\nAuthorized recipients can read and retain copies; revocation cannot recall bytes.\nSigned records prove authenticity, not the truth of the content.',48,377,865,87,17)
s.finish('Native Spaces and broader delegated execution remain future work.',('ADR 0011 · ATProto overview and Spaces alpha in notes',ADR),f'The selected adapter uses AT Protocol identity, schemas and signed repositories for authenticated publication/distribution. Memory governs reviewed publication, disclosure, authorization, approved recipients/versions, ordering/history and private Q&A. Private questions and answers use Memory’s addressed gateway mailbox. They never enter the PDS, shared publication log, general project timeline or retrieval corpus. The deployment trusts gateway policy enforcement and availability. Authorized administrators/recipients can see plaintext and retain copies; revocation denies subsequent gateway operations; already authorized in-flight work may complete. Previously delivered copies remain readable while local authorization permits and cannot be recalled. Identity signatures do not guarantee factual accuracy. The selected gateway architecture does not depend on experimental native Spaces. Spaces is alpha and broader delegated execution/F2 remains future work.\nSources: {ADR}\n{F1}\nhttps://atproto.com/guides/overview\nhttps://atproto.com/blog/atproto-spaces-alpha')

s=Slide(16,'Appendix · evidence without a quality claim')
s.box(48,140,410,184,BLUE);s.text('60',65,157,372,72,48,True);s.text('Knowledge answers\nawaiting genuine blinded review',65,248,372,65,21)
s.box(502,140,410,184,PEACH);s.text('0 / 15',519,157,372,72,48,True);s.text('Successful complete\ncoding sequences',519,248,372,65,21)
s.text('R3 development run · 17 September 2026',48,349,865,37,22,True)
s.text('All 75 executions completed. Harness validity and quality did not pass.\nNo validated quality improvement, perfect recall or token-saving claim.',48,401,865,64,18)
s.finish('Controlled federation demonstrations establish bounded behavior, not customer ROI.',('R3 development continuation and coding diagnosis',R3),f'Latest R3 development continuation dated 17 September 2026 completed 75/75 executions. Sixty knowledge answers await genuine blinded review; successful complete coding sequences are 0/15. These are three repeats of one coding repository family under five conditions, not 15 independent tasks. Report flags: execution_valid=true, harness_valid=false, official_evidence=false, quality_passed=false. Execution completion is not task success or validated accuracy. Coding diagnosis identifies harness/tool/budget issues as well as actual coding failures; this does not change the 0/15 result. Old headline claims of 18.1% task success, perfect recall and 41% fewer tokens are removed. No quality improvement or customer operational gain has been validated.\nSources: {R3}\n{BASE}evals/r3/coding-diagnosis-20260917.md\n{F1}')

s=Slide(17,'Appendix · Base commercial forecast')
s.table([['Shared economics · GBP','Year 1','Year 2','Year 3'],['Year-end ARR','£160k','£900k','£2.24m'],['Recognized revenue','£42k','£552k','£1.60m'],['Paying accounts (expected)','2.00','10.40','22.38'],['London / NY gross margin','−80.8% / −101.4%','52.1% / 48.0%','74.2% / 72.7%']],48,130,864,240,[297,189,189,189],font=17)
s.text('Pilot funnel: starts from month 5, two pilot months, 50% conversion.\nNet revenue retention: (1 − 5% churn) × (1 + 45% expansion) = 137.75%.',48,393,866,70,18)
s.finish('36 months from 1 October 2026. Fractional accounts are expected values, not signed customers.',('M2 v3-assurance · Base · live source check 17 September 2026',LON),f'Financial preparation, not a validated commercial forecast. Year 1 Oct 2026–Sep 2027; Year 2 Oct 2027–Sep 2028; Year 3 Oct 2028–Sep 2029. Shared £80,000 annual commitment with two deployments, no Base pilot/onboarding/overage receipts; annual cash invoicing at start/renewal and service-year revenue recognition. The original provisional quarterly acquisition volumes are pilot-start hypotheses, starting month five conditional on readiness, with two pilot months and 50% conversion. Annual churn5% and retained-account expansion45% imply 137.75% NRR. Fractional customer counts represent expected values. London and New York share commercial economics, converted at planning FX $1.3476/£, 14 September 2026; margins differ due to regional costs and are calculated, not preset.\nLondon Summary!C6:E11 and B33; Assumptions!E21:E27, E95:E109 (pilot volumes and NRR): {LON}\nNew York Summary!C6:E11 and B33: {NY}\nModel M2-2026-09-15-v3-assurance; fingerprint fnv1a32:d74d6a28. Captures dated 15 September 2026; live inputs, formulas and Base selection rechecked 17 September 2026. See M4 financial-evidence register for precise captured values and ranges.')

s=Slide(18,'Appendix · alternative operating bases')
s.table([['24-month funding objective','London · GBP','New York · USD'],['Gross funding requirement','£3.23m','$4.91m'],['Expected employee AI spend','£420k','$566k'],['AI headroom retained as cash','£420k','$566k'],['M25–27 operating buffer','£504k','$744k']],48,127,864,231,[390,237,237],font=18)
s.text('New York gross funding in common GBP: £3.65m at $1.3476/£.',48,376,864,35,18)
s.text('Fund to month 24; retain AI headroom plus months 25–27 operating outflows.\nAlternative bases, never combined. External assurance costs remain unquoted.',48,423,864,47,17)
s.finish('AI headroom is cash, not a second expense. These scenarios do not determine a round size.',('M2 Summary, Employee AI and funding schedules · details in notes',LON),f'Base gross funding: London £3,226,268.299476148; NY $4,913,192.66445569, equivalent £3,645,883.5444165114 at planning FX1.3476. Net funding £3,161,742.933486625 / $4,814,928.811166576; fees £64,525.365989523 / $98,263.853289114. The objective avoids negative cash through month24 and ends month24 with months25–27 operating outflows plus unused first24month employee AI allowance. London required month24 cash £924,025.0675705 = £504,025.0675705 buffer + £420,000 AI headroom. NY $1,309,757.209858 = $743,765.209858 buffer + $565,992 AI headroom. Expected first24month AI spending is separately £420,000 / $565,992. Full36month AI spending and unused allowance are each £690,000 / $929,844. Headroom stays cash, not a provision or supplier expense.\nSources: London Summary!B22:B29 (funding), B32 (common GBP), B73:B74 (24-month AI expense/headroom), Summary!C19:E19 (annual AI); Employee AI!C26:AL28 (36-month totals): {LON}\nNew York same ranges: {NY}\n{MODEL}\nAssurance lead and fractional counsel are included. Full external ISO27001/ISO42001 certification and SOC2TypeII audit/advisory costs remain unquoted; retained legacy one-off amounts do not cover their full scope. This is a 24-month funding objective, not guaranteed36month runway or profitability. Combined launch-delay/doubled-support/full-AI stress can produce negative month36 cash. Models are alternative company bases, never summed. No fixed fundraising ask. M3 validates actual costs, capacity and assurance quotes; M5 supplies customer evidence.')
(OUT/'content.json').write_text(json.dumps(slides,ensure_ascii=False,indent=2))
(OUT/'object-manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2))
print(json.dumps({'slides':len(slides),'requests':sum(len(json.loads((OUT/f'slide-{n:02}.json').read_text())) for n in range(1,19)),'reused':sum(e['action']=='reuse' for s in manifest for e in s['elements']),'preserved':sum(e['action']=='keep' for s in manifest for e in s['elements'])}))
