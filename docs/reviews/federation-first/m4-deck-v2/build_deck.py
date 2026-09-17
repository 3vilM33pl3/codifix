#!/usr/bin/env python3
"""Generate one-time native edits for the dated compact-v2 copy. No network calls.

Input: full untouched native copy JSON, original M4 object manifest, output directory.
Reuse inspected native exemplars/objects; preserve each logo and page background.
Not idempotent: later edits require fresh native JSON and a revision guard.
"""
import copy, json, pathlib, re, sys
SOURCE, OLD_MANIFEST, OUT = map(pathlib.Path, sys.argv[1:4]); OUT.mkdir(parents=True, exist_ok=True)
HERE=pathlib.Path(__file__).parent

def unwrap(x):
    if isinstance(x, dict):
        if isinstance(x.get('slides'), list): return x
        for v in x.values():
            y=unwrap(v)
            if y:return y
    if isinstance(x,list):
        for v in x:
            y=unwrap(v)
            if y:return y
    if isinstance(x,str):
        try:return unwrap(json.loads(x))
        except ValueError:pass
p=unwrap(json.loads(SOURCE.read_text()))
old=json.loads(OLD_MANIFEST.read_text())
LOGOS={s['pageId']:{e['id'] for e in s['elements'] if e['action']=='keep' and e['role']=='Codifix native logo'} for s in old}
INK='1A1A1A'; MUTED='61565D'; PINK='F7D9E6'; BLUE='DEE4F2'; PEACH='F7E3D9'; WHITE='FFFFFF'
DOSSIER='https://app.notion.com/p/3de8990715f48198a8f3f9a4ac34092d'
M1='https://app.notion.com/p/3dc8990715f4815a812bf2a9262ef282'
M2='https://app.notion.com/p/3dc8990715f4813fb281d4a8175c3d1a'
LON='https://docs.google.com/spreadsheets/d/1PIUTGbZ2a-7sXsxAl6wbChJVTeKN8Q2K2rv1LRcBbO0/edit'
NY='https://docs.google.com/spreadsheets/d/1fT9Ta-IjX65NqwcRXFRJq5tW814aqje2Vha9YFfx2es/edit'
DB='https://docs.databricks.com/aws/en/ai-gateway/register-mcp-service'
LEX='https://atproto.com/specs/lexicon'
F1='docs/reviews/federation-first/f1-execution.md'
ADR='docs/developer/adr/0011-atproto-federation-adapter.md'
R3='evals/r3/development-continuation-20260917.md'
SNAPSHOT='Repository evidence checked 17 September 2026 at 54195f4. Concurrent uncommitted engineering is excluded.'
FIT={'m4_s2_text_2': (0, -3.6), 'm4_s2_text_4': (0, -3.6), 'm4_s4_text_3': (0, -3.6), 'm4_s4_text_6': (0, -3.6), 'm4_s4_text_7': (0, -3.6), 'm4_s4_text_8': (0, -7.2), 'p8_i12': (-7.2, 0), 'm4_s8_text_1': (-7.2, -7.2), 'm4_s15_text_3': (0, -3.6), 'm4_s15_text_4': (0, -3.6)}
slides=[];manifest=[]
def rgb(h):return {k:int(h[i:i+2],16)/255 for k,i in [('red',0),('green',2),('blue',4)]}
def tx(e):return ''.join(t.get('textRun',{}).get('content','') for t in e.get('shape',{}).get('text',{}).get('textElements',[]))

class Slide:
    def __init__(self,n,pid,title):
        self.n=n;self.pid=pid;self.orig=copy.deepcopy(next(s for s in p['slides'] if s['objectId']==pid));self.elements=self.orig['pageElements']
        self.used=set(LOGOS[pid]);self.req=[];self.serial=0;self.items=[];self.mapping=[]
        titleid=next(e['objectId'] for e in self.elements if e['objectId'] not in self.used and tx(e).strip())
        self.text(title,48,37,864,86,32,True,id=titleid,role='title')
    def fresh(self,kind):
        self.serial+=1;return f'v2_s{self.n}_{kind}_{self.serial}'
    def get(self,kind,id=None):
        if id:e=next(e for e in self.elements if e['objectId']==id)
        else:
            e=next((e for e in self.elements if e['objectId'] not in self.used and (
                (kind=='text' and 'shape' in e and tx(e).strip()) or
                (kind=='box' and 'shape' in e and not tx(e).strip() and e['shape']['shapeType'] in ['RECTANGLE','ROUND_RECTANGLE']) or
                (kind=='line' and 'line' in e))),None)
        if e:self.used.add(e['objectId'])
        return e
    def pos(self,e,x,y,w,h):
        unit=e['size']['width']['unit'];f=12700 if unit=='EMU' else 1
        sx=w/(e['size']['width']['magnitude']/f);sy=h/(e['size']['height']['magnitude']/f)
        if 'table' in e:sx=sy=1
        self.req.append({'updatePageElementTransform':{'objectId':e['objectId'],'applyMode':'ABSOLUTE','transform':{'scaleX':sx,'scaleY':sy,'translateX':x,'translateY':y,'unit':'PT'}}})
    def text(self,t,x,y,w,h,size=21,bold=False,color=INK,align='START',id=None,role='body',link=None):
        e=self.get('text',id);oid=e['objectId'] if e else self.fresh('text')
        dx,dy=FIT.get(oid,(0,0));x+=dx;y+=dy
        if e:
            # M4 text frames are uniform styles. Logo mixed styles are protected.
            runs=[v.get('textRun',{}).get('style',{}) for v in e['shape']['text']['textElements'] if v.get('textRun',{}).get('content','').strip()]
            if len({json.dumps(v,sort_keys=True) for v in runs})>1:raise ValueError(f'Mixed styles require range mapping: {oid}')
            self.req.append({'deleteText':{'objectId':oid,'textRange':{'type':'ALL'}}});self.pos(e,x,y,w,h)
        else:self.req.append({'createShape':{'objectId':oid,'shapeType':'TEXT_BOX','elementProperties':{'pageObjectId':self.pid,'size':{'width':{'magnitude':w,'unit':'PT'},'height':{'magnitude':h,'unit':'PT'}},'transform':{'scaleX':1,'scaleY':1,'translateX':x,'translateY':y,'unit':'PT'}}}})
        self.req.extend([
          {'insertText':{'objectId':oid,'insertionIndex':0,'text':t}},
          {'deleteParagraphBullets':{'objectId':oid,'textRange':{'type':'ALL'}}},
          {'updateShapeProperties':{'objectId':oid,'shapeProperties':{'shapeBackgroundFill':{'propertyState':'NOT_RENDERED'},'outline':{'propertyState':'NOT_RENDERED'},'contentAlignment':'TOP','autofit':{'autofitType':'NONE'}},'fields':'shapeBackgroundFill,outline,contentAlignment,autofit.autofitType'}},
          {'updateTextStyle':{'objectId':oid,'textRange':{'type':'ALL'},'style':{'fontFamily':'Arial','fontSize':{'magnitude':size,'unit':'PT'},'bold':bold,'italic':False,'underline':False,'foregroundColor':{'opaqueColor':{'rgbColor':rgb(color)}},**({'link':{'url':link}} if link else {})},'fields':'fontFamily,fontSize,bold,italic,underline,foregroundColor,link'}},
          {'updateParagraphStyle':{'objectId':oid,'textRange':{'type':'ALL'},'style':{'alignment':align,'lineSpacing':105,'spaceAbove':{'magnitude':0,'unit':'PT'},'spaceBelow':{'magnitude':0,'unit':'PT'},'indentStart':{'magnitude':0,'unit':'PT'},'indentEnd':{'magnitude':0,'unit':'PT'},'indentFirstLine':{'magnitude':0,'unit':'PT'}},'fields':'alignment,lineSpacing,spaceAbove,spaceBelow,indentStart,indentEnd,indentFirstLine'}}])
        item={'id':oid,'action':'reuse' if e else 'create','role':role,'text':t,'bounds':[x,y,w,h],'fontSize':size,'link':link}
        self.items.append(item);self.mapping.append(item);return oid
    def box(self,x,y,w,h,color=BLUE):
        e=self.get('box');oid=e['objectId'] if e else self.fresh('box')
        if e:self.pos(e,x,y,w,h)
        else:self.req.append({'createShape':{'objectId':oid,'shapeType':'ROUND_RECTANGLE','elementProperties':{'pageObjectId':self.pid,'size':{'width':{'magnitude':w,'unit':'PT'},'height':{'magnitude':h,'unit':'PT'}},'transform':{'scaleX':1,'scaleY':1,'translateX':x,'translateY':y,'unit':'PT'}}}})
        self.req.extend([{'updateShapeProperties':{'objectId':oid,'shapeProperties':{'shapeBackgroundFill':{'solidFill':{'color':{'rgbColor':rgb(color)},'alpha':1}},'outline':{'propertyState':'NOT_RENDERED'}},'fields':'shapeBackgroundFill,outline'}},{'updatePageElementsZOrder':{'pageElementObjectIds':[oid],'operation':'SEND_TO_BACK'}}])
        self.mapping.append({'id':oid,'action':'reuse' if e else 'create','role':'semantic panel','bounds':[x,y,w,h]});return oid
    def line(self,x,y,x2,y2,arrow=False,color=MUTED):
        e=self.get('line');oid=e['objectId'] if e else self.fresh('edge')
        if e:self.pos(e,x,y,x2-x,y2-y)
        else:self.req.append({'createLine':{'objectId':oid,'lineCategory':'STRAIGHT','elementProperties':{'pageObjectId':self.pid,'size':{'width':{'magnitude':max(abs(x2-x),1),'unit':'PT'},'height':{'magnitude':max(abs(y2-y),1),'unit':'PT'}},'transform':{'scaleX':0 if x2==x else (1 if x2>x else -1),'scaleY':0 if y2==y else (1 if y2>y else -1),'translateX':x,'translateY':y,'unit':'PT'}}}})
        self.req.append({'updateLineProperties':{'objectId':oid,'lineProperties':{'lineFill':{'solidFill':{'color':{'rgbColor':rgb(color)},'alpha':1}},'weight':{'magnitude':1.5,'unit':'PT'},'dashStyle':'SOLID','startArrow':'NONE','endArrow':'FILL_ARROW' if arrow else 'NONE'},'fields':'lineFill,weight,dashStyle,startArrow,endArrow'}})
        self.mapping.append({'id':oid,'action':'reuse' if e else 'create','role':'connector' if arrow else 'rule','endpoints':[x,y,x2,y2]})
    def table(self,values,x,y,widths,h):
        e=next(e for e in self.elements if 'table' in e);oid=e['objectId'];self.used.add(oid)
        assert (len(values),len(values[0]))==(e['table']['rows'],e['table']['columns'])
        self.pos(e,x,y,sum(widths),h)
        for j,w in enumerate(widths):self.req.append({'updateTableColumnProperties':{'objectId':oid,'columnIndices':[j],'tableColumnProperties':{'columnWidth':{'magnitude':w,'unit':'PT'}},'fields':'columnWidth'}})
        self.req.append({'updateTableRowProperties':{'objectId':oid,'rowIndices':list(range(len(values))),'tableRowProperties':{'minRowHeight':{'magnitude':h/len(values),'unit':'PT'}},'fields':'minRowHeight'}})
        for i,row in enumerate(values):
            for j,t in enumerate(row):
                loc={'rowIndex':i,'columnIndex':j}
                self.req.extend([{'deleteText':{'objectId':oid,'cellLocation':loc,'textRange':{'type':'ALL'}}},{'insertText':{'objectId':oid,'cellLocation':loc,'insertionIndex':0,'text':t}},{'updateTextStyle':{'objectId':oid,'cellLocation':loc,'textRange':{'type':'ALL'},'style':{'fontFamily':'Arial','fontSize':{'magnitude':18,'unit':'PT'},'bold':i==0,'foregroundColor':{'opaqueColor':{'rgbColor':rgb(INK)}}},'fields':'fontFamily,fontSize,bold,foregroundColor'}},{'updateParagraphStyle':{'objectId':oid,'cellLocation':loc,'textRange':{'type':'ALL'},'style':{'lineSpacing':100,'spaceAbove':{'magnitude':0,'unit':'PT'},'spaceBelow':{'magnitude':0,'unit':'PT'}},'fields':'lineSpacing,spaceAbove,spaceBelow'}}])
        item={'id':oid,'action':'reuse','role':'native financial table','values':values,'bounds':[x,y,sum(widths),h]};self.mapping.append(item)
    def finish(self,status,source,notes):
        if status:self.text(status,48,472,864,30,15,False,MUTED,role='evidence label')
        self.text(source[0],48,507,720,20,10,False,MUTED,role='source link',link=source[1])
        self.text(f'{self.n:02}',780,506,32,21,10,False,MUTED,role='slide number')
        self.req.append({'updatePageElementsZOrder':{'pageElementObjectIds':[t['id'] for t in self.items]+list(LOGOS[self.pid]),'operation':'BRING_TO_FRONT'}})
        nid=self.orig['slideProperties']['notesPage']['notesProperties']['speakerNotesObjectId']
        notes=notes+'\n\n'+SNAPSHOT+'\nDated compact-v2 evidence: '+DOSSIER
        self.req.extend([{'deleteText':{'objectId':nid,'textRange':{'type':'ALL'}}},{'insertText':{'objectId':nid,'insertionIndex':0,'text':notes}}])
        for match in re.finditer(r'https://[^\s]+',notes):
            url=match.group(0).rstrip('.,;');start=len(notes[:match.start()].encode('utf-16-le'))//2;end=start+len(url.encode('utf-16-le'))//2
            self.req.append({'updateTextStyle':{'objectId':nid,'textRange':{'type':'FIXED_RANGE','startIndex':start,'endIndex':end},'style':{'link':{'url':url}},'fields':'link'}})
        for e in self.elements:
            if e['objectId'] not in self.used:
                self.req.append({'deleteObject':{'objectId':e['objectId']}});self.mapping.append({'id':e['objectId'],'action':'delete','reason':'Unused exemplar content or structure in compact narrative','oldText':tx(e)})
            elif e['objectId'] in LOGOS[self.pid]:self.mapping.append({'id':e['objectId'],'action':'keep','role':'Codifix native logo'})
        narrative=' '.join(t['text'] for t in self.items if t['role']=='body')
        slides.append({'number':self.n,'pageId':self.pid,'title':self.items[0]['text'],'items':self.items,'notesId':nid,'notes':notes,'narrativeWords':len(narrative.split())})
        manifest.append({'number':self.n,'pageId':self.pid,'elements':self.mapping})
        (OUT/f'slide-{self.n:02}.json').write_text(json.dumps(self.req,ensure_ascii=False,indent=2))

s=Slide(1,'p1','Infrastructure for the\nAgentic Enterprise')
s.text('Codifix is the plumbing and electrical wiring\nfor the Agentic Enterprise.',48,163,850,98,30,True)
s.text('Your platforms keep their interfaces.\nCodifix does the integration.',48,309,850,81,25,True)
s.finish('Connect existing AI platforms, agents and enterprise expertise.',('Infrastructure vision · evidence and boundaries',DOSSIER),'''Founder-selected positioning: Codifix supplies the infrastructure for an Agentic Enterprise. The plumbing and electrical wiring phrase is a metaphor for reusable connections, knowledge and governance. The goal is easier and lower-risk transformation; no measured risk reduction, universal connector coverage or productivity claim is implied. Knowledge/governance remains the foundation, broad platform integration the product direction. Platform-native adapters allow external systems to keep supported interfaces; Codifix builds the integration and maps authorization explicitly. Configuration, authentication and permission mapping remain necessary. No new standard must be adopted by the external platform. This is an investor discussion deck, not an offer or fixed fundraising ask. The Tom briefing is pre-meeting research only; no conversation, feedback, endorsement or investment interest is claimed.''')

s=Slide(2,'p2','Great AI platforms.\nA disconnected enterprise.')
for x,c,title,body in [(48,BLUE,'Application team','AI platform + agents\nChange history and symptoms'),(554,PEACH,'Platform team','AI platform + agents\nRunbooks and infrastructure context')]:
    s.box(x,161,358,183,c);s.text(title,x+18,180,322,40,22,True,role='diagram label');s.text(body,x+18,242,322,83,21)
s.line(420,254,540,254,True);s.text('Handover',416,211,126,33,18,False,MUTED,'CENTER',role='diagram label')
for x,w,t in [(48,284,'Context stays local'),(351,265,'Connections rebuilt'),(652,271,'Controls rechecked')]:s.text(t,x,390,w,66,21,True,role='diagram label')
s.finish('Illustrative problem to validate in discovery.',('Buyer and incident-handover hypothesis · M1',M1),'''This is a fictional enterprise structure, not a customer case study. Native AI capabilities do not alone establish authorized, reusable expertise exchange across independently administered units. Discovery must confirm actual fragmentation, repeated connector/governance work, expert interruption and budget ownership. Do not imply existing platforms lack connectors, enterprise search, memory or governance. Codifix's proposed focus is exchange across independent administrations and corrections owned/reviewed at origin. Use the incident handover from application to platform teams as the first bounded proof. Source: M1 preparation, docs/reviews/federation-first/m1-execution.md. No numerical market, productivity or customer-traction claims.''')

s=Slide(3,'p4','Your platforms keep their interfaces.\nCodifix connects them.')
for x,c,title in [(48,BLUE,'Application unit'),(586,PEACH,'Platform unit')]:
    s.box(x,151,326,282,c)
    s.text(title,x+17,167,292,37,22,True,role='diagram label')
    s.text('Existing platform + agents\nNative APIs / MCP',x+17,221,292,64,20,role='diagram label')
    chip=s.box(x+17,301,292,46,WHITE)
    s.req.append({'updatePageElementsZOrder':{'pageElementObjectIds':[chip],'operation':'BRING_TO_FRONT'}})
    s.text('Codifix adapter',x+24,306,275,37,22,True,role='diagram label')
    s.line(x+163,283,x+163,298,True)
    s.text('Own knowledge\nOwn administration',x+17,362,292,62,20,role='diagram label')
s.text('Authorized exchange\nMemory gateway',382,192,196,76,18,True,align='CENTER',role='diagram label')
s.line(389,311,571,311,True);s.line(571,333,389,333,True)
s.text('ATProto identity\nSigned repositories',382,359,196,65,18,align='CENTER',role='diagram label')
s.finish('Architecture direction: Codifix builds adapters and maps permissions.',('Adapter architecture · ATProto Lexicon',LEX),f'''Architectural direction: external platforms use their own supported interfaces; Codifix implements adapters. The diagram does not establish that every API/MCP platform has a shipped connector or that credentials/permissions transfer automatically. Vertices are independently administered units, each with local knowledge and agents. Authorized edges exchange selected expertise. ATProto sits between Codifix network components, not as a new interface requirement imposed on Databricks or every external platform. Private Q&A still uses the Memory gateway; it is not placed in the PDS or publication log. ATProto Lexicons describe records, RPC endpoints and events; XRPC defines the wire conventions and service authentication. These extension points do not provide arbitrary enterprise-policy semantics. Memory owns disclosure, authorization and operational history. Sources: {LEX}; https://atproto.com/specs/xrpc; {ADR}; docs/developer/architecture/mcp-server.md. Controlled implementation and integration strategy are distinct.''')

s=Slide(4,'p5','Databricks plugs into the network')
s.text('Proposed integration using supported interfaces',48,112,864,40,19,True,MUTED,role='evidence label')
for x,c,title,body in [(48,BLUE,'Databricks agent','Native MCP\nIncident question'),(354,PINK,'Codifix','Adapter + controls\nPermitted request'),(660,PEACH,'Platform expertise','Private Q&A\nReviewed sources')]:
    s.box(x,176,252,143,c);s.text(title,x+13,192,226,38,22,True,role='diagram label');s.text(body,x+13,248,226,63,20,role='diagram label')
s.line(312,247,342,247,True);s.line(618,247,648,247,True)
s.line(782,338,178,338,True);s.text('Cited answer returns to the existing agent',215,346,551,35,20,True,align='CENTER',role='diagram label')
s.text('Payment latency: check the permitted runbook.\nA responder acts; the source team reviews corrections for later reuse.',48,397,864,66,21)
s.finish('No platform rewrite or ATProto adoption. Connector configuration still required.',('Databricks external MCP documentation · integration not yet verified',DB),f'''Illustrative consumer path: Databricks agent → Unity Catalog registered MCP Service and HTTP connection → Codifix Streamable HTTP MCP interface → permitted private Memory expertise Q&A → cited answer. Databricks is a consumer, not a live federated data source in this example. The incident and expected answer are fictional; no deployment, affiliation or customer outcome is claimed. No Databricks-specific connector or end-to-end test was found. No platform rewrite or ATProto adoption is required by this proposed route, but endpoint reachability, TLS/network configuration, scoped credential mapping and a consent/citation test remain necessary.
Databricks requires Streamable HTTP, a Unity Catalog enabled workspace, supported model-serving region and reachability from serverless compute over permitted public/private networking. Invocation requires EXECUTE and USE CATALOG/SCHEMA; restrict USE CONNECTION to service authors/admins because direct connection use can bypass service restrictions. Stored Bearer credentials are supported, alongside OAuth options. Shared credentials do not propagate a person's identity or authorize onward model disclosure. Confirm target cloud/account availability. Native agent-use documentation is Public Preview; service policies have their own Beta status. Do not imply the entire integration path is GA.
Memory has scoped Bearer/x-api-token HTTP MCP and per-request roles. F1 requires exact-question consent plus operator-bound caller/output destination; read_only=false for ask. An adapter must preserve these requirements, including human confirmation. Correction reuse requires origin review, local curation and renewed collection review; ordinary source memory is not automatically rewritten.
Sources: {DB}; https://docs.databricks.com/aws/en/agents/mcp-tools/use-mcp-in-agents; https://docs.databricks.com/aws/en/query-federation/http#authentication; https://docs.databricks.com/aws/en/ai-gateway/govern-mcp-service; docs/developer/architecture/mcp-server.md; docs/user/f1-internal-expertise.md; {F1}.''')

s=Slide(5,'p6','Build the infrastructure once.\nExtend transformation.')
for i,(a,b) in enumerate([('Reuse connections','Less repeated integration effort'),('Reuse reviewed knowledge','Faster correct action; fewer interruptions'),('Keep explicit controls','Lower disclosure and rework risk')]):
    y=159+i*93;s.text(a,48,y,365,62,22,True);s.text(b,445,y,467,62,21)
    if i<2:s.line(48,y+75,912,y+75)
s.text('Measure onboarding effort, correct action, failures and full delivery cost.',48,440,864,35,20)
s.finish('Value hypotheses. Recovered staff capacity is not automatically cash saved.',('Pilot measurement plan · M1',M1),'''Reusable infrastructure is intended to preserve platform investments and reduce repeated connection/control work as teams extend AI transformation. Each new interface still needs scoped adapter work, security/permission mapping and upkeep. No one-time integration or zero-maintenance promise. Values are hypotheses to establish in M3/M5, not realized ROI. Measure onboarding elapsed time and effort, handover-to-first-correct-action and accepted resolution, expert interruption minutes, approved-correction reuse, recurrence, quality, useful completion, refusals/failures and inappropriate disclosure. Count infrastructure, inference, curation, administration, support and integration upkeep. Match baseline and assisted cases by severity, service, novelty and responder familiarity with comparable information/resources. Retain failed/refused cases. Missing thresholds or insufficient comparable cases prevent a success declaration. Released capacity is not cash saving unless actual spending changes; do not count interruption savings twice. Source: M1 pilot charter/measurement tables.''')

s=Slide(6,'p12','Working foundations.\nThe next proof points.')
for i,(label,title,body) in enumerate([('Today','Memory interfaces, federation and private Q&A','Implemented components; controlled demonstrations.'),('Next','A verified Databricks connection','Permissions, consent, citations and recovery tested end to end.'),('Then','A measured customer pilot','Quality, operating value and delivery economics.')]):
    y=148+i*106;s.text(label,48,y,112,44,20,True,MUTED,role='diagram label');s.text(title,181,y,722,39,22,True);s.text(body,181,y+47,722,51,20)
s.finish('Controlled evidence does not establish customer readiness or quality improvement.',('Dated implementation and R3 evaluation evidence',DOSSIER),f'''Implemented: stdio/Streamable HTTP MCP; scoped per-request service authorization; reviewed federation publication/import; version/provenance controls; private gateway Q&A and explicit onward-consent boundary. F1 uses synthetic data, fake provider synthesis and independent databases/HTTP applications in a single test process on one host. The E gate starts independent service executables. These are controlled demonstrations, not customer deployments, customer SSO or a multi-host fleet. Full security, recovery, deployment and evaluation gates remain.
R3 development observation on 17 September: 75/75 executions completed; 60 knowledge answers await genuine blinded human review; 0/15 successful complete coding sequences (one repository family, five conditions, three repeats). execution_valid=true, harness_valid=false, official_evidence=false, quality_passed=false. Deadline/tool/completion-protocol failures do not isolate Memory's coding contribution. No validated quality improvement, perfect recall or token saving is claimed.
Since prior M4, committed protected payload export/import operator and metadata-only area-supervisor inspection add bounded foundations. They do not establish real-data area import, integrated agent execution, release activation or a live four-area demo. Native Spaces adoption and broader delegated execution remain future work.
Sources: {F1}; docs/reviews/federation-first/2026-09-09-e-completion.md; {R3}; evals/r3/coding-diagnosis-20260917.md; docs/reviews/four-area-network/README.md; docs/developer/architecture/mcp-server.md.''')

s=Slide(7,'p8','Start with two teams.\nExpand across the enterprise.')
s.text('Buyer',48,143,369,31,18,True,MUTED,role='diagram label');s.text('CIO / CTO',48,183,371,42,25,True)
s.text('Champions',48,257,369,31,18,True,MUTED,role='diagram label');s.text('Application + platform leaders',48,297,376,65,21,True)
s.text('£80k / year · two deployments',48,395,406,66,22,True,role='diagram label')
for i,(title,body,c) in enumerate([('Start','Two independent teams',BLUE),('Expand','Divisions and acquisitions',PEACH),('Extend','Authorized partners',PINK)]):
    y=145+i*92;s.box(496,y,416,79,c);s.text(title,512,y+6,384,30,21,True,role='diagram label');s.text(body,512,y+40,384,33,20,role='diagram label')
s.text('8-week pilot: 2 baseline / 4 assisted / 2 analysis',496,438,416,37,16,role='diagram label')
s.finish('Pricing and expansion hypotheses; commercial qualification plus technical readiness.',('Buyer, pilot charter and Base packaging · M1 / M2',M1),f'''Economic buyer: CIO/CTO or technology-operations leader; application/platform champions. Founder-led discovery qualifies recurring pain, measurable operational cost, independent control, sponsor/budget access, baseline feasibility and expansion. Internal discovery priority 9/12 is provisional, not an evidence-based threshold. A pilot needs two independently administered instances, named owners, approved sharing, observable baseline and passed technical gates. Illustrative eight weeks: two baseline, four assisted, two analysis; calendar completion is not success if cases or thresholds are missing.
£80,000 annual platform package for one account with two independently administered deployments remains an unvalidated price anchor. Additional deployments, onboarding and usage are separate hypotheses; Base includes no pilot/onboarding/overage receipts. Replicated copies are not separately billable new knowledge. M2 pilot funnel starts month 5 conditional on readiness, two pilot months and 50% conversion; net retention=(1−5% churn)×(1+45% retained expansion)=137.75%. Fractional accounts are expected values, not signed customers. Expansion to divisions, acquisitions and authorized partners is a strategy, not commitments.
Sources: {M1}; {M2}; London Assumptions!E12:E27 {LON}; same shared assumptions in New York {NY}.''')

s=Slide(8,'p14','Fund the next proof points')
s.text('Olivier Van Acker · creator of Memory Layer',48,126,864,37,22,True)
s.text('Rust platform, MCP integration and evaluation tooling',48,166,864,36,20)
for i,(title,body,c) in enumerate([('Repeatable integrations','Connect existing platforms with explicit controls.',BLUE),('Measurable customer value','Prove one workflow with a qualified pilot.',PEACH),('Viable delivery economics','Validate cost, support and onboarding effort.',PINK)]):
    y=222+i*65;s.box(48,y,864,54,c);s.text(title,62,y+8,343,41,21,True,role='diagram label');s.text(body,430,y+8,467,41,20)
s.text('Discuss the milestones and financing structure.',48,439,864,37,22,True)
s.finish('Planned: nine employees + fractional counsel. Round size follows scope and evidence.',('Founder evidence and planned operating model',DOSSIER),'''Founder credibility is based on shipped repository work: Olivier Van Acker created the Rust Memory Layer platform, code-graph retrieval, MCP integration and evaluation tooling. No invented career metrics, employer logos, candidate pipeline or team hiring commitment. Planned nine employees include founder plus a Head of Security & AI Governance; fractional counsel is external. Roles cover core/platform/retrieval engineering, deployment, enterprise/security, commercial and support. Assurance leadership owns preparation for ISO 27001, ISO 42001 and SOC 2 Type II; no certification or completed attestation claimed. Employee AI allowance £5,000/person/month at 50% utilisation; actual spend and throughput remain hypotheses. Financial details in appendix. Capital should demonstrate repeatable platform connections, customer-agreed quality and operating outcomes, and cost/onboarding economics. No fixed ask. The existing M2 scenarios do not establish the budget for universal connector coverage or Neuralese; scope and costing must precede a round-size decision. M3 and M5 remain open. Sources: repository README and implementation at snapshot; M2 model Team/Employee AI schedules; M1 pilot preparation.''')

s=Slide(9,'m4_appendix_15','Architecture today. Communication tomorrow.')
for x,c,title,body in [(48,BLUE,'ATProto foundation','Identity + signed records\nExtensible schemas and services'),(494,PEACH,'Memory governance','Authorization + publication history\nPrivate Q&A through the gateway')]:
    s.box(x,133,418,132,c);s.text(title,x+15,145,388,38,22,True,role='diagram label');s.text(body,x+15,196,388,60,19)
s.text('Trust boundaries',48,292,864,31,21,True,role='diagram label')
s.text('Trusted gateway; authorized copies remain readable.\nRevocation cannot recall copies. Signatures do not establish truth.',48,334,864,58,19)
s.text('Neuralese · future research',48,408,864,31,22,True,role='diagram label')
s.text('Machine-native agent communication: compatibility, evaluation and auditability first.',48,447,864,33,19)
s.finish('',('ATProto specifications · Memory boundaries · Neuralese research',DOSSIER),f'''ATProto supplies stable identifiers, record schemas, signed repositories and synchronization; custom Lexicon services extend the protocol. It does not automatically supply enterprise policy, truth, immutable audit history or cross-model representation compatibility. Memory enforces authorization, reviewed publication/version history and disclosure. Private expertise Q&A uses the addressed gateway mailbox, outside the PDS, shared publication log and retrieval corpus. Gateway availability/enforcement are trusted dependencies. Authorized gateways/operators/recipients can read and retain content. Revocation denies subsequent gateway operations; already authorized in-flight work may complete and delivered copies cannot be recalled while local authorization permits reading.
Native ATProto Spaces is upstream alpha; Codifix adoption remains deferred, and broader delegated execution remains future work. Do not portray these as completed or production ready.
Neuralese is an application-level research direction: learned/latent inter-agent representations. ATProto can carry extensible data/messages but transport flexibility does not make heterogeneous model representations compatible. No latent-payload feature, token saving, superior quality or Neuralese security guarantee is implemented. Any experiment needs supported participant interfaces, representation compatibility, human-review/audit design, workload/cost and task-quality evaluation. Structured messages are not automatically Neuralese.
Sources: {LEX}; https://atproto.com/specs/xrpc; https://atproto.com/specs/did; https://atproto.com/specs/repository; https://atproto.com/specs/sync; https://atproto.com/blog/atproto-spaces-alpha; {ADR}; {F1}; docs/developer/architecture/federation-security.md; docs/future-development/federation-protocol-spike.md:106; https://aclanthology.org/P17-1022/ (Translating Neuralese, primary 2017 research).''')

s=Slide(10,'m4_appendix_18','Operating model and funding scenarios')
s.table([['24-month funding objective','London · GBP','New York · USD'],['Gross funding requirement','£3.23m','$4.91m'],['Expected employee AI spend','£420k','$566k'],['AI headroom retained as cash','£420k','$566k'],['M25–27 operating buffer','£504k','$744k']],48,124,[400,232,232],221)
s.text('Planned: nine employees + fractional counsel; assurance lead included.',48,365,864,32,19)
s.text('New York in common GBP: £3.65m at $1.3476/£. Alternative bases.',48,408,864,33,19)
s.text('AI headroom is cash. External assurance and broader connector/research costs need validation.',48,453,864,44,16,False,MUTED,role='evidence label')
s.finish('',('M2 Base · live source check 17 Sep 2026 · ranges and limits in notes',LON),f'''These are alternative company bases serving the same global workload; never add them. All values are planning hypotheses, not a fundraising ask, hiring/spending commitment, customer evidence or assurance quote.

Live verification, 17 September 2026, 12:37:56–12:37:59 UTC: both remain Base, M2-2026-09-15-v3-assurance, formula fingerprint fnv1a32:d74d6a28. Each location has 12,354 matching formulas, 371 inputs matching capture signatures, 25 matching Base outputs and 23 passing reconciliations; 304 shared inputs agree. Original captures remain 15 September at 20:38:21.652Z London / 20:38:16.635Z New York. This was verification, not a new scenario refresh.

Source ranges in London / New York:
Summary!B23 gross funding: £3,226,268.30 / $4,913,192.66.
Summary!B22 net funding: £3,161,742.93 / $4,814,928.81; B24 financing fees: £64,525.37 / $98,263.85.
Summary!B73 employee AI expense through month 24: £420,000 / $565,992. B74 earmarked AI cash headroom: the same amounts, additional retained cash rather than an expense.
Summary!B25 operating cash outflows for months 25–27: £504,025.07 / $743,765.21. B27 month-24 closing cash: £924,025.07 / $1,309,757.21.
Summary!B32 New York common-GBP funding: £3,645,883.54 at Assumptions!E11 planning FX $1.3476/£, dated 14 September 2026.
Summary!B58 first-24-month counsel fees: £120,000 / $180,000. Team!B46 assurance lead annual salary: £130,000 / $190,000; AM46 hire month 3; AQ46 included. Summary!C18:E18 year-end staffing: 8 / 9 / 9, including founder; counsel is external.

Employee AI policy: Assumptions!E119 allowance £5,000 per person per month, E120 utilisation 50%, E121 subscription component £250. Expected expense £2,500 comprises £250 subscriptions plus £2,250 metered usage; do not charge included subscription usage twice. Employee AI!C9:C12 shows the policy. Allowance starts at hire; fractional counsel is excluded. Full-36-month expected spending and headroom are each £690,000 ($929,844), separately disclosed from the 24-month funding objective.

Headroom remains cash, not a second expense or supplier payment. Required net funding avoids negative cash through month 24 and retains months 25–27 operating outflows plus the first 24 months of unused AI allowance at month 24, with opening cash, financing timing and fees shown separately. This does not guarantee 36-month runway.

Full initial and recurring external ISO 27001, ISO 42001 and SOC 2 Type II costs remain unquoted (Summary!B70; Assumptions!B130). No separately costed universal-adapter or Neuralese programme exists. Payroll/AI envelopes do not establish incremental engineering, compatibility, compute or support coverage. M3 must validate scope, packaging, provider rates, actual employee AI spend, throughput/rework, support capacity and legal/assurance quotes. M5 supplies customer evidence.
Workbooks: {LON}
{NY}
Financial evidence: {M2}.''')

for name,obj in [('slide-content.json',slides),('object-manifest.json',manifest)]:
    (HERE/name).write_text(json.dumps(obj,ensure_ascii=False,indent=2))
order=[s['pageId'] for s in slides]
(OUT/'cleanup.json').write_text(json.dumps([{'deleteObject':{'objectId':s['objectId']}} for s in p['slides'] if s['objectId'] not in order]))
(OUT/'order.json').write_text(json.dumps([{'updateSlidesPosition':{'slideObjectIds':[pid],'insertionIndex':0}} for pid in reversed(order)]))
print(json.dumps({'sourceRevision':p['revisionId'],'slideIds':order,'narrativeWords':{s['number']:s['narrativeWords'] for s in slides},'requests':{s['number']:len(json.loads((OUT/f"slide-{s['number']:02}.json").read_text())) for s in slides}},indent=2))
