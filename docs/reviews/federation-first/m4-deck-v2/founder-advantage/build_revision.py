#!/usr/bin/env python3
"""One-time native slide 8 revision; no API calls. Requires the pre-founder snapshot."""
import pathlib,json,sys,copy
ROOT=pathlib.Path(__file__).parent.parent
SOURCE=ROOT/'self-improving'
helper=(ROOT/'build_deck.py').read_text().split('\ns=Slide(1,')[0]
args=sys.argv[:]
sys.argv=[str(ROOT/'build_deck.py'),args[1],str(SOURCE/'object-manifest.json'),args[2]]
g={'__file__':str(ROOT/'build_deck.py')};exec(compile(helper,str(ROOT/'build_deck.py'),'exec'),g);sys.argv=args
assert g['p']['revisionId']=='PpuOu-E_XqasQg','Unexpected source revision'
g['FIT']={};g['SNAPSHOT']='Founder career wording checked 17 September 2026 against the existing CV/archive and founder confirmations. Product, evaluation and financial evidence retain their previously recorded dates.'
Slide=g['Slide']
def fresh(self,kind):
 self.serial+=1;return f'v2founder_s{self.n}_{kind}_{self.serial}'
Slide.fresh=fresh
s=Slide(8,'p14','Enterprise experience.\nA network to open doors.')
s.text('Olivier Van Acker — founder and creator of Memory Layer',48,126,864,38,22,True)
s.text('Shipped Rust platform, MCP integration and evaluation tooling',48,166,864,32,20)
for x,color,title,body in [(48,g['BLUE'],'Enterprise delivery','BAE Systems client engagement\nShell engineering leadership\nMcKinsey technology consulting'),(494,g['PEACH'],'Established relationships','A route to introductions\nInsight into enterprise buyers\nFounder-led discovery')]:
 s.box(x,216,418,137,color)
 s.text(title,x+14,224,390,34,22,True,role='section label')
 s.text(body,x+14,266,390,84,20)
s.text('Next proof points',48,367,864,30,18,True,g['MUTED'],role='diagram label')
s.text('Repeatable integrations',48,401,295,33,20,True,role='diagram label')
s.text('Measured pilots',364,401,233,33,20,True,role='diagram label')
s.text('Validated delivery economics',611,401,303,33,20,True,role='diagram label')
s.text('Discuss milestones and financing.',48,441,864,34,22,True)
s.finish('Planned: nine employees + fractional counsel. Round size follows scope and evidence.',('Founder experience, relationships and delivery plan',g['DOSSIER']),'''Olivier Van Acker's founder advantage combines experience delivering inside complex enterprises, established professional relationships and the ability to build the product. This is a rationale for reaching and serving buyers, not a guarantee of access, adoption or commercial success.
Enterprise delivery: BAE Systems is a founder-confirmed client engagement, explicitly confirmed in this conversation on 17 September 2026. No BAE job title, dates, quantified outcome, direct-employment relationship or current Codifix client status is asserted. Shell: Global Head of Engineering (contract), March 2018–October 2019, with global engineering and MLOps capability responsibilities. McKinsey & Company: Digital Consultant (contract), February 2015–August 2016, covering software delivery, technology modernisation and hands-on cloud/architecture capability. These claims come from the founder's existing CV, pages 3–4, and career archive; they are not independent employer verification. Company names identify prior experience and do not imply endorsement.
Career source: /home/olivier/Documents/Olivier Van Acker Resume.pdf, pages 3–4; /home/olivier/Documents/Olivier Van Acker Resume - 20260123.odt. Accessible archive and exact submitted CV: https://app.notion.com/p/3dd8990715f48194b40ff4d4583735a8 .
Relationships: the founder selected “established relationships” as the accurate description on 17 September. A route to introductions, buyer insight and founder-led discovery is the commercial strategy; no named buyer has been represented as interested, no pilot discussions or qualified pipeline are asserted, and no company endorsement or guaranteed warm introduction is implied. Historical connection map: https://app.notion.com/p/3b78990715f480c98005c07ebed4a3e9 . That exercise was last reviewed 19 August, sent no outreach and did not have the underlying export available for rechecking. Do not reuse stale counts, current-role assumptions or warmth ratings.
Why this matters: experience navigating large organisations can inform problem discovery, stakeholder alignment, procurement/security conversations and adoption planning. Established relationships can provide a route to relevant conversations. These advantages must turn into qualified evidence: two independent owners, sponsorship, approved sharing, baseline and technical readiness, followed by measured customer outcomes. Prior employers/clients are not described as prospective Codifix customers.
Product credibility remains shipped Memory Layer work: Rust platform, MCP interfaces and evaluation tooling. Existing claims and limitations on the other slides are unchanged. The next proof points are a repeatable, verified Databricks integration including permissions/consent/citations/recovery; a measured qualified incident-handover pilot; and validated costs, support and onboarding economics. M3 and M5 remain open, including validation accuracy, missed changes, review effort, validation-token costs and correction reuse. No fixed fundraising ask.
Planned organisation remains nine employees including founder, plus external fractional counsel. The assurance lead coordinates ISO 27001, ISO 42001 and SOC 2 Type II preparation; there is no completed certification/attestation claim. Employee AI and all financial assumptions remain unchanged. Round size follows evidence and scope, including separately validated integration/research costs. Sources: existing M4 product evidence; M1 pilot preparation; M2 operating model. Financial source: https://app.notion.com/p/3dc8990715f4813fb281d4a8175c3d1a .''')
content=json.loads((SOURCE/'slide-content.json').read_text());manifest=json.loads((SOURCE/'object-manifest.json').read_text());content[7]=g['slides'][0];manifest[7]=g['manifest'][0]
out=pathlib.Path(args[2]);(out/'slide-content.json').write_text(json.dumps(content,ensure_ascii=False,indent=2));(out/'object-manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2))
print(json.dumps({'slide':8,'requests':len(s.req),'narrativeWords':content[7]['narrativeWords']},indent=2))
