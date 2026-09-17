#!/usr/bin/env python3
"""One-time revision request builder. No network calls. Use only with pre-revision native snapshot."""
import pathlib,json,sys,copy,re
ROOT=pathlib.Path(__file__).parent.parent
# Load only the reusable native authoring helpers. Never execute original deck construction.
helper=(ROOT/'build_deck.py').read_text().split("\ns=Slide(1,")[0]
saved_argv=sys.argv[:]
sys.argv=[str(ROOT/'build_deck.py'),saved_argv[1],str(ROOT/'object-manifest.json'),saved_argv[2]]
g={'__file__':str(ROOT/'build_deck.py')}
exec(compile(helper,str(ROOT/'build_deck.py'),'exec'),g)
sys.argv=saved_argv
assert g['p']['revisionId']=='eo7sVBXZEaBxxA', 'Unexpected pre-revision snapshot'
g['FIT']={}
g['SNAPSHOT']='Knowledge-maintenance, retrieval and review evidence checked 17 September 2026 at 5827aeb. Existing federation/R3 evidence and financial checks retain their original dated observations.'
Slide=g['Slide'];DOSSIER=g['DOSSIER'];MUTED=g['MUTED']
def fresh(self,kind):
 self.serial+=1
 return f'v2r2_s{self.n}_{kind}_{self.serial}'
Slide.fresh=fresh
old_content=json.loads((ROOT/'slide-content.json').read_text())
old_manifest=json.loads((ROOT/'object-manifest.json').read_text())
s=Slide(5,'p6','Self-improving knowledge,\nwith human control')
for x,c,title,body in [(48,g['BLUE'],'Prioritize','Usage + source volatility'),(350,g['PINK'],'Gather evidence','Recorded sources + history'),(654,g['PEACH'],'Decide','Evidence + confidence')]:
 s.box(x,145,258,82,c)
 s.text(title,x+10,152,238,35,22,True,role='diagram label')
 s.text(body,x+10,194,238,30,17,role='diagram label')
s.line(312,178,342,178,True);s.line(612,178,646,178,True)
# A decision fans out to three alternative outcomes.
s.line(783,228,783,246);s.line(172,246,783,246)
for x in [172,477,783]:s.line(x,246,x,263,True)
s.text('Reconfirm',48,273,258,38,22,True,role='diagram label')
s.text('Supported knowledge',48,315,258,35,20,role='diagram label')
s.text('Flag uncertainty',350,273,258,38,22,True,role='diagram label')
s.text('Insufficient evidence',350,315,258,35,20,role='diagram label')
s.text('Review a correction',654,273,270,38,22,True,role='diagram label')
s.text('New version',654,315,258,35,20,role='diagram label')
s.line(672,351,672,371,True)
s.text('Reuse later',654,374,258,35,20,role='diagram label')
# Future notification is visually separated from implemented decision outcomes.
s.line(477,354,477,412)
s.req[-1]['updateLineProperties']['lineProperties']['dashStyle']='DASH'
s.mapping[-1]['dashStyle']='DASH'
s.text('Next: notify the responsible human.',48,422,864,35,21,True,role='roadmap label')
s.finish('Implemented and configurable. Quality improvement remains to be measured.',('Knowledge maintenance, evidence and review boundaries',DOSSIER),'''Codifix seeks supporting evidence and preserves reviewable history as knowledge changes. This is implemented, configurable knowledge maintenance, not measured autonomous quality improvement. The diagram shows the enabled, non-dry-run decision paths; it does not claim every check rewrites content or requires human approval.
Prioritize: retrieval, citations and direct reads reinforce activation. This usage signal identifies frequently used knowledge; it is not proof of correctness. Volatility is an EWMA of recorded source-verification status changes, not detection of every changing enterprise fact. Higher volatility brings eligible rechecks forward. Activation thresholds, an existing review flag, cooldown, due time and daily/per-cycle budgets still apply. High volatility with low activation does not trigger evidence gathering or an LLM call.
Automatic validation defaults to off, with dry-run on and auto-apply rewording off. Operators must opt in. Dry-run records evidence and would_* outcomes/cooldown without the diagram's live content/status changes. Source: crates/mem-config/src/pipeline.rs:537; crates/mem-reinforce/src/selection.rs:35; crates/mem-service/src/repository/handlers/reinforcement.rs:216.
Gather evidence: bounded recorded source snippets, provenance verification, one-hop related memories, prior validations and source-path repository history. A strict evidence-reference allowlist rejects unknown references; accepted references do not prove conclusions or calibrate model confidence. Optional manual/API hybrid fallback scans a bounded tracked repository when first-pass evidence is ambiguous or unsupported. No unrestricted web research or mathematical proof. Source: crates/mem-reinforce/src/validate/evidence.rs:71; verdict.rs:131; mod.rs:291.
Decide: clear valid knowledge refreshes validation metadata without a new content version. Ambiguous, unsupported or low-confidence findings set needs_review and leave content unchanged. Flag-only findings are surfaced by scores --needs-review/query flags, not the default pending correction queue. A supported proposed correction is queued separately; an authorized reviewer can apply or reject it. Approval writes an immutable new version while preserving canonical identity/history. The review permission is a service role, not proof of a human identity. Neither flags nor queued proposals currently notify/contact anyone outside Memory. The dashed Next branch is planned proactive human outreach, requiring ownership, channel permissions, acknowledgement and audit design.
Weak evidence does not authorize a rewrite. Explicitly enabled, high-confidence wording-only changes can auto-apply; separate curation policies have their own replacement rules. Do not claim all changes require human approval. Source: crates/mem-reinforce/src/validate/apply.rs:76; mod.rs:350,573; crates/mem-curate/src/repository.rs:570; docs/user/cli/review.md. Cross-unit correction reuse still requires origin/local curation and renewed collection review. Validation quality, missed changes, reviewer time, validation-token cost and subsequent correction reuse remain M3/M5 evidence gates. Architecture explanation: docs/developer/architecture/memory-reinforcement.md.''')

s=Slide(6,'p12','Connected context. Portable memory.\nTraceable corrections.')
for y,title,body in [(157,'Connected retrieval','Meaning, keywords and code relationships\nfind relevant context.'),(260,'Portable memory','Knowledge persists across supported agent clients\nand embedding-model changes.'),(363,'Traceable corrections','Sources, version history and review decisions\nremain connected.')]:
 s.text(title,48,y,303,65,22,True,role='diagram label')
 s.text(body,374,y,538,69,21)
s.finish('Implemented strengths. Their combined customer value remains to be measured.',('Retrieval, embedding spaces, review and evaluation evidence',DOSSIER),'''Codifix's distinctive combination is connected retrieval, portable canonical memory and traceable corrections. This describes product focus, not exclusivity or a claim that competitors lack these features.
Connected retrieval: the query pipeline combines semantic similarity, lexical/keyword matching and bounded graph signals. The code graph must be extracted first; memories link through file provenance to symbols and nearby relationships. Semantic retrieval needs populated embeddings. Graph connections explain retrieval but are not standalone answer citations. Answer synthesis exposes evidence sufficiency and validates cited memory IDs. The current retrieval channels run sequentially, so no parallel-retrieval performance claim is made. Sources: docs/user/cli/query.md:41; crates/mem-search/src/lib.rs:377.
Portable memory: canonical content and history are separate from derived vectors. Supported agent clients can use Memory's MCP interfaces. Embedding spaces are keyed by provider, base URL and model; several spaces can coexist. A new space must be populated/backfilled and validated before active use. Switching to an already populated supported space need not recompute existing vectors. Multi-backend creation incurs additional embedding calls. Provider support for embeddings does not imply identical chat-model support, interchangeable database engines or universal platform/model compatibility. Sources: docs/user/cli/embeddings.md; crates/mem-search/src/embedding_backend.rs:37; rebuild.rs:113; lib.rs:150; docs/developer/architecture/mcp-server.md.
Traceable corrections: canonical memory identity connects provenance, review outcomes and immutable versions. Approved factual corrections create a new version; explicitly configured high-confidence wording changes and separate curation policy differ. Signatures establish authenticity/integrity, not factual truth or human approval. Sources: docs/user/cli/review.md; crates/mem-reinforce/src/validate/mod.rs:573; docs/developer/adr/0002-reinforcement-scores-outside-immutable-versions.md.
''' + old_content[5]['notes'].split('\n\nRepository evidence checked')[0])

# Slide 8 keeps native geometry/style. Change only two body strings and add milestones to notes.
slide8=copy.deepcopy(old_content[7]);req=[]
for oid,text in [('p14_i7','Validate a Databricks connection end to end.'),('p14_i10','Measure quality and value in a qualified pilot.')]:
 item=next(i for i in slide8['items'] if i['id']==oid);item['text']=text
 req.extend([{'deleteText':{'objectId':oid,'textRange':{'type':'ALL'}}},{'insertText':{'objectId':oid,'insertionIndex':0,'text':text}}])
notes=slide8['notes']+'\n\nNext proof points carried from slide 6: verify the proposed Databricks connection end to end, including permissions, exact-question consent, citations and recovery. Pass deployment, security, recovery and evaluation gates before customer operation. Then measure a qualified incident-handover pilot against agreed baseline/thresholds. M3/M5 must establish validation accuracy, missed source changes, review effort, validation-token costs and approved-correction reuse. These add evidence requirements, not a changed funding model or a claim of quality gain. Proactive human notification remains future work.'
slide8['notes']=notes
nid=slide8['notesId'];req.extend([{'deleteText':{'objectId':nid,'textRange':{'type':'ALL'}}},{'insertText':{'objectId':nid,'insertionIndex':0,'text':notes}}])
for match in re.finditer(r'https://[^\s]+',notes):
 url=match.group(0).rstrip('.,;');start=len(notes[:match.start()].encode('utf-16-le'))//2;end=start+len(url.encode('utf-16-le'))//2
 req.append({'updateTextStyle':{'objectId':nid,'textRange':{'type':'FIXED_RANGE','startIndex':start,'endIndex':end},'style':{'link':{'url':url}},'fields':'link'}})
out=pathlib.Path(saved_argv[2]);(out/'slide-08.json').write_text(json.dumps(req,indent=2))
content=copy.deepcopy(old_content);manifest=copy.deepcopy(old_manifest)
for n in [5,6]:
 content[n-1]=next(x for x in g['slides'] if x['number']==n)
 manifest[n-1]=next(x for x in g['manifest'] if x['number']==n)
content[7]=slide8
for item in manifest[7]['elements']:
 replacement=next((i for i in slide8['items'] if i['id']==item['id']),None)
 if replacement:item.update(replacement)
(out/'slide-content.json').write_text(json.dumps(content,ensure_ascii=False,indent=2))
(out/'object-manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2))
print(json.dumps({'slides':[5,6,8],'requestFiles':[str(out/f'slide-{n:02}.json') for n in [5,6,8]],'newNarrativeWords':{x['number']:x['narrativeWords'] for x in g['slides']}},indent=2))
