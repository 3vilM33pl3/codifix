#!/usr/bin/env python3
"""Build the shared M2 native Google Sheets model; emits requests, never writes remotely.

Source legacy snapshots are immutable evidence. Forecast assumptions are hypotheses.
Run: python3 build_model.py [--region london|newyork|both] [--output-dir /tmp]
"""
from __future__ import annotations
import argparse, datetime as dt, hashlib, json, pathlib, re

ROOT = pathlib.Path(__file__).resolve().parent
VERSION = 'M2-2026-09-15-v2-ai'
TODAY = '2026-09-15'
NAVY = {'red':.059,'green':.137,'blue':.235}
TEAL = {'red':.0,'green':.42,'blue':.42}
WHITE = {'red':1,'green':1,'blue':1}
BLUE = {'red':.10,'green':.27,'blue':.77}
GREEN = {'red':.0,'green':.40,'blue':.22}
INK = {'red':.12,'green':.16,'blue':.22}
PALE = {'red':.91,'green':.96,'blue':.96}
GRAY = {'red':.42,'green':.47,'blue':.53}
NUM = '#,##0.00;(#,##0.00);"–"'
INT = '#,##0;(#,##0);"–"'
PCT = '0.0%;(0.0%);"–"'
MONEY = '#,##0;(#,##0);"–"'
DATE = 'mmm-yy'


def col(n):
    result = ''
    while n:
        n, rem = divmod(n-1,26)
        result = chr(65+rem)+result
    return result


def serial(date):
    return (date-dt.date(1899,12,30)).days


def source_cells(data):
    result = {}
    for sheet in data['sheets']:
        name = sheet['properties']['title']
        for grid in sheet.get('data',[]):
            for ri, row in enumerate(grid.get('rowData',[]),grid.get('startRow',0)+1):
                for ci, cell in enumerate(row.get('values',[]),grid.get('startColumn',0)+1):
                    v = cell.get('userEnteredValue',{})
                    if v:
                        result[f'{name}!{col(ci)}{ri}'] = next(iter(v.values()))
    return result


def quoted(sheet, address):
    return f"'{sheet}'!{address}"


class Model:
    def __init__(self, region):
        self.region = region
        self.legacy = json.loads((ROOT/f'legacy-{region}.json').read_text())
        self.metadata = json.loads((ROOT/f'metadata-{region}.json').read_text())
        self.src = source_cells(self.legacy)
        self.currency = 'GBP' if region=='london' else 'USD'
        self.ids = {s['properties']['title']:s['properties']['sheetId'] for s in self.metadata['sheets']}
        self.original_ids = dict(self.ids)
        self.ids.update({'Costs':2040000001,'Cohorts':2040000002,'Checks':2040000003,'Employee AI':2040000004})
        self.cells = {name:{} for name in self.ids}
        self.formulas = {}
        self.editable = []
        self.notes = {}
        self.row_formats = {}
        self.assumptions = {}
        self.controls = []
        self.base_controls = {}

    def set(self, sheet, r, c, value, fmt=None, editable=False, note=None):
        address = f'{col(c)}{r}'
        if isinstance(value,str) and value.startswith('='):
            # Native Sheets removes redundant quotes around simple sheet identifiers.
            # Canonicalize references only outside formula string literals so the
            # refresh compares exact native formulas without weakening verification.
            parts = re.split(r'("(?:[^"]|"")*")', value)
            value = ''.join(part if i % 2 else re.sub(r"'([A-Za-z_][A-Za-z0-9_]*)'!", r'\1!', part) for i, part in enumerate(parts))
            val = {'formulaValue':value}
            self.formulas[f'{sheet}!{address}'] = value
            color = GREEN if '!' in value and sheet not in ('Summary','Sensitivity','Checks') else INK
        elif isinstance(value,bool):
            val = {'boolValue':value}; color=BLUE if editable else INK
        elif isinstance(value,(int,float)):
            val = {'numberValue':value}; color=BLUE if editable else INK
        else:
            val = {'stringValue':str(value)}; color=BLUE if editable else INK
        cell={'userEnteredValue':val,'userEnteredFormat':{'textFormat':{'foregroundColor':color}}}
        if fmt:
            cell['userEnteredFormat']['numberFormat']={'type':'DATE' if fmt==DATE else 'NUMBER','pattern':fmt}
        if note:cell['note']=note
        self.cells[sheet][(r,c)] = cell
        if editable:self.editable.append(f'{sheet}!{address}')

    def f(self, sheet,r,c,formula,fmt=None):self.set(sheet,r,c,'='+formula,fmt)

    def textrow(self, sheet,r,values):
        for c,v in enumerate(values,1):
            if v is not None:self.set(sheet,r,c,v)

    def a(self,key):return f"Assumptions!$E${self.assumptions[key]}"

    def add_driver(self,key,row,label,base,unit,source='M2 planning hypothesis; validate in M3–M5',down=None,up=None,note='',fmt=None):
        self.assumptions[key]=row
        vals=[base,base if down is None else down,base if up is None else up]
        self.set('Assumptions',row,1,label)
        for c,v in enumerate(vals,2):
            self.set('Assumptions',row,c,v,fmt=fmt or (PCT if unit in ('ratio','share','annual ratio') else DATE if unit=='date' else NUM),editable=True,note=note)
            self.controls.append(f'Assumptions!{col(c)}{row}')
            self.base_controls[f'Assumptions!{col(c)}{row}']=v
        self.f('Assumptions',row,5,f'IF(ISNUMBER(INDEX(B{row}:D{row},1,MATCH($B$3,$B$8:$D$8,0))),INDEX(B{row}:D{row},1,MATCH($B$3,$B$8:$D$8,0)),NA())',fmt=fmt or (PCT if unit in ('ratio','share','annual ratio') else DATE if unit=='date' else NUM))
        for c,v in enumerate([unit,source,TODAY,'Founder / finance owner','Hypothesis'],6):self.set('Assumptions',row,c,v)
        if source.startswith('Approved M2'):
            self.set('Assumptions',row,10,'Approved planning convention; unvalidated economics')

    def setup(self):
        r='Assumptions'
        self.textrow(r,1,['CODIFIX | M2 FINANCIAL ASSUMPTIONS'])
        self.textrow(r,2,['Blue inputs are editable. E selects one scenario. No customer, price or provider quote has been validated.'])
        self.set(r,3,1,'Selected case');self.set(r,3,2,'Base',editable=True)
        self.controls.insert(0,'Assumptions!B3');self.base_controls['Assumptions!B3']='Base'
        self.textrow(r,4,['Formula model version',VERSION])
        self.textrow(r,5,['Company base','London' if self.region=='london' else 'New York'])
        self.textrow(r,6,['Reporting currency',self.currency,None,None,'=IF(B6="GBP",1,E11)'])
        self.textrow(r,7,['Accounting basis','Planning; pre-tax, no VAT/sales tax, debt, dividends or opening receivables/deferred balances. Annual service accrual; monthly payroll/cost payment.'])
        self.textrow(r,8,['Driver','Base','Downside','Upside','Active','Unit','Source / rationale','As of','Owner','Evidence status'])
        add=self.add_driver
        approved='Approved M2 plan, 2026-09-15'
        add('start',10,'Forecast start',serial(dt.date(2026,10,1)),'date',approved)
        add('fx',11,'USD per GBP',1.3476,'USD / GBP','Bank of England 14 Sep 2026 planning snapshot; https://www.bankofengland.co.uk/boeapps/database/Rates.asp?into=GBP&rateview=A',down=1.20,up=1.50,fmt='0.0000')
        add('package',12,'Annual platform package',80000,'GBP / account / year',approved,down=64000,up=96000,note='Unvalidated package commitment, not legacy operation usage. 45% annual expansion below is price/commitment expansion; workload has a separate growth input.')
        add('includeddeploy',13,'Deployments included in package',2,'deployments / account',approved)
        add('deploy',14,'Actual independently administered deployments',2,'deployments / active account',approved)
        add('adddeployfee',15,'Additional deployment annual fee',10000,'GBP / deployment / year',note='Illustrative incremental platform fee; replicated copies of knowledge are never billed as new knowledge.')
        add('onboardingfee',16,'Onboarding fee at paid conversion',0,'GBP / new paying account',approved)
        add('pilotfee',17,'Pilot fee billed at pilot start',0,'GBP / pilot',approved,note='Recognised across pilot duration; pilot fees do not count toward ARR.')
        add('includedq',18,'Included service questions',1000,'questions / paid account / month',note='Illustrative service question allowance; not interchangeable with legacy operations or provider tokens.')
        add('questions',19,'Eligible service questions per paid account',500,'questions / account / month',down=750,up=500,note='Workload hypothesis. Includes refused and failed attempts; no customer throughput evidence.')
        add('overage',20,'Uncommitted question overage price',0,'GBP / question',approved,note='Base no overage receipts. Overage billed one month after service and excluded from ARR.')
        add('firstpilot',21,'Earliest pilot start month',5,'month index',approved)
        add('delay',22,'Additional launch delay',0,'months',approved,down=3)
        add('ready',23,'Readiness permission for scenario (0 / 1)',1,'0 or 1',note='1 is a conditional forecast convention, not a claim readiness has passed. R3, recovery, security and customer approvals remain separate gates.')
        add('duration',24,'Pilot service duration',2,'months',approved)
        add('conversion',25,'Pilot to paid conversion',.50,'ratio',approved,down=.25,up=.65)
        add('churn',26,'Annual logo churn at renewal',.05,'annual ratio','Legacy Assumptions B13; separate logo attrition at each annual renewal',down=.10,up=.03)
        add('expansion',27,'Retained account commitment expansion',.45,'annual ratio','Legacy Assumptions B12; commercial renewal hypothesis, not workload evidence',down=.15,up=.50)
        add('pilotmult',28,'Pilot start volume multiplier',1,'multiplier',down=.75,up=1.25)
        add('lag',29,'Invoice to receipt delay',1,'whole months',approved,down=3)
        add('financemonth',30,'Financing arrival month',1,'month index',note='Financing cannot cure an earlier cash deficit; Summary and Checks flag timing infeasibility.')
        add('fees',31,'Financing fee',.02,'ratio',note='Provisional percentage of gross proceeds; no financing arrangement or advisor quote.')
        add('openingcash',32,'Opening unrestricted cash',0,f'{self.currency}',note='Placeholder assumption, not an assertion about bank balances.')
        add('legacyraise',33,'Historical net raise reference',self.src['Assumptions!B58'],self.currency,'Legacy Assumptions B58; comparison only, not funding target')
        add('usagegrowth',34,'Annual service workload growth',.45,'annual ratio',note='Separate from monetary contract expansion. Applied from each cohort paid start; pilot workload stays at its base volume.')
        add('failure',35,'Initial question failure/refusal share',.10,'share',down=.20,up=.05,note='Service planning assumption; all initial questions cost inference and failed/refused cases remain in workload.')
        add('retries',36,'Additional attempts per initial failure',1,'attempts / failed question')
        add('tokens',37,'Memory inference tokens per attempt',4000,'tokens / attempt',note='Blended input/output workload hypothesis; not a legacy operation mapping.')
        add('tokenprice',38,'Blended Memory inference cost',2.5,'GBP / million tokens',note='Illustrative blended planning rate; no provider quote or model selection.')
        add('infra',39,'Application infrastructure per deployment',80,'GBP / deployment / month')
        add('gateway',40,'Gateway and PDS cost per deployment',35,'GBP / deployment / month')
        add('identity',41,'Identity and key infrastructure',5,'GBP / deployment / month')
        add('gbcollection',42,'Origin collection size',10,'GB / collection')
        add('collections',43,'Collections per deployment',3,'collections / deployment')
        add('replicas',44,'Additional stored replicas per origin collection',1,'copies / collection',note='Storage copies only; zero effect on billable new knowledge or ARR.')
        add('storage',45,'Stored data cost',.08,'GBP / GB / month')
        add('transfergb',46,'Transfer per attempt',.0005,'GB / attempt')
        add('transferprice',47,'Transfer cost',.06,'GBP / GB')
        add('backupprice',48,'Backup storage and recovery reserve',.04,'GBP / stored GB / month')
        add('recoveryinfra',49,'Recovery test infrastructure',4,'GBP / deployment / month')
        add('securityhours',50,'Customer security review effort',8,'hours / new pilot account')
        add('deployhours',51,'Deployment onboarding effort',8,'hours / new deployment')
        add('collectionhours',52,'Initial collection review effort',2,'hours / new collection')
        add('correctionshare',53,'Questions requiring curation',.02,'share')
        add('correctionhours',54,'Origin-reviewed correction effort',.25,'hours / correction')
        add('adminhours',55,'Administration effort',1,'hours / deployment / month')
        add('supporthours',56,'Multi-party support effort',3,'hours / active account / month')
        add('failhours',57,'Additional support per initial failed question',.03,'hours / failed question')
        add('keyhours',58,'Identity and key operation effort',.25,'hours / deployment / month')
        add('recoveryhours',59,'Recovery exercise effort',.5,'hours / deployment / month')
        add('contractor',60,'Delivery contractor cost',75,'GBP / hour',note='Purchased only for delivery hours above allocated salaried capacity; no double count of every hour plus salary.')
        add('productivehours',61,'Delivery capacity per allocated FTE',120,'hours / FTE / month')
        add('serviceowner',62,'Codifix share of Memory service costs',1,'share',approved,note='Applies to external Memory delivery costs, delivery salary allocation and delivery workload. Unallocated salaries remain operating expenses.')
        add('agentcost',63,'Customer agent runtime reference spend','PENDING','GBP / active account / month',note='Unmeasured; excluded from Codifix by default. Populate a numeric rate before assigning any Codifix ownership.')
        add('agentowner',64,'Codifix share of customer runtime spending',0,'share',approved)
        add('fixedinfra',65,'Shared delivery infrastructure',150,'GBP / month',note='Charged from month one even before revenue; does not include product development software tools.')
        add('directmult',66,'External delivery rate multiplier',1,'multiplier',down=1.50,up=.90)
        add('supportmult',67,'Delivery effort multiplier',1,'multiplier',down=1.50,up=.90)
        add('salarymult',68,'Regional salary multiplier',1,'multiplier',down=1.10)
        add('hiredelay',69,'Hiring delay versus source schedule',0,'months')
        local=[('load',70,42,'Loaded salary factor','multiplier'),('commission',71,43,'New ARR commission','ratio'),('recruit',72,44,'Recruitment fee on agency base salary','ratio'),('agency',73,45,'Share of agency hires','share'),('tools',74,46,'Software and tools per employee',f'{self.currency} / FTE / month'),('office',75,47,'Workspace per employee',f'{self.currency} / FTE / month'),('marketing',76,48,'Marketing from month 4',f'{self.currency} / month'),('events',77,49,'Events from month 6',f'{self.currency} / month'),('legal',78,50,'Legal, accounting and payroll',f'{self.currency} / month'),('soc2',79,51,'SOC 2 programme planning expense',self.currency),('soc2month',80,52,'SOC 2 expense month','month index'),('insurance',81,53,'Insurance',f'{self.currency} / month'),('travel',82,54,'Travel from month 6',f'{self.currency} / month'),('contingency',83,55,'Other operating expense contingency','ratio')]
        for key,row,old,label,unit in local:add(key,row,label,self.src[f'Assumptions!B{old}'],unit,f'Legacy {self.region} Assumptions B{old}; unvalidated local planning input')
        add('capex',84,'Additional cash capital expenditure',0,f'{self.currency} / month',note='Base zero. Employee kit already included in legacy loaded salary factor. This is a pre-tax planning forecast, not a full statutory statement.')
        add('pilotqshare',85,'Pilot question workload versus paid account',.50,'share')
        add('extraopeningAR',86,'Opening receivables',0,self.currency,note='Zero opening balance convention; change requires an opening receivables collection schedule, outside this model version.')
        add('extraopeningdeferred',87,'Opening deferred revenue',0,self.currency,note='Zero opening balance convention; change requires remaining performance obligations, outside this model version.')
        add('phase1',88,'First month share of quarterly pilot starts',0,'share','Legacy quarterly acquisition phasing')
        add('phase2',89,'Second month share of quarterly pilot starts',.4,'share','Legacy quarterly acquisition phasing')
        add('phase3',90,'Third month share of quarterly pilot starts',.6,'share','Legacy quarterly acquisition phasing')
        self.set(r,11,8,'2026-09-14')
        # Unknown customer runtime spend is an explicit text status, never zero cost evidence.
        self.f(r,63,5,'INDEX(B63:D63,1,MATCH($B$3,$B$8:$D$8,0))')
        for rr in [86,87]:
            for cc in [2,3,4]:
                address=f'Assumptions!{col(cc)}{rr}'
                self.editable.remove(address);self.controls.remove(address);self.base_controls.pop(address)
                self.cells[r][(rr,cc)]['userEnteredFormat']['textFormat']['foregroundColor']=INK
        self.textrow(r,91,['PILOT START PLAN | source volumes shifted when launch is delayed'])
        for i,v in enumerate([0,1,3,4,4,4,5,5,6,7,7,8]):
            add(f'q{i+1}',95+i,f'Pilot starts in source quarter {i+1}',v,'expected pilots / quarter','Legacy Assumptions B17:B28 repurposed as unvalidated pilot starts')
        self.textrow(r,109,['Derived net revenue retention',None,None,None,f'=(1-E26)*(1+E27)','annual ratio','Derived from the same anniversary renewal logic used by Cohorts.'])
        self.textrow(r,110,['Annual commitment per new account',None,None,None,'=(E12+MAX(0,E14-E13)*E15)*E6',self.currency,'No charge for replicated copies.'])
        self.textrow(r,112,['INTERPRETATION','Readiness=1 assumes future gate clearance for the forecast. It does not authorise a customer pilot. Fractional account forecasts are expectations.'])
        self.textrow(r,113,['COST OWNERSHIP','Customer runtime reference spend is separated from Codifix costs. Customer capacity value and cash savings are outside these company models.'])
        self.textrow(r,114,['EXPANSION','Commitment expansion increases renewal contract value; workload growth is independently editable in row 34. No automatic workload equivalence is assumed.'])
        add('staffing',118,'Staffing profile: 0 lean / 1 historical',0,'0 or 1',approved,note='0 uses editable Team AM hire and AQ inclusion; 1 uses preserved original AR hire and AS inclusion. Historical comparison uses the same live forecast and AI policy.')
        add('employeeallowance',119,'Employee AI allowance',5000,'GBP / employee / month',approved,note='Founder-selected operating hypothesis. Subscriptions and metered usage share one allowance. No provider quote, spending approval or productivity validation.')
        add('employeeutil',120,'Employee AI expected utilisation',.50,'share',approved,note='Expected expense = allowance × utilisation; unused first 24-month allowance remains earmarked cash in the funding target, never an expense.')
        add('employeesubscription',121,'Employee AI subscription allocation ceiling',250,'GBP / employee / month',approved,note='Allocation ceiling within expected spending, not a fixed executed subscription commitment. MIN(ceiling, allowance × utilisation); subscription-included usage is excluded from incremental metered charges.')
        self.textrow(r,123,['AGENTIC ENGINEERING','The lean eight-person team and GBP 5000 allowance at 50% utilisation are founder-selected hypotheses. M3 must measure engineering throughput, review/rework, AI spending and delivery capacity.'])
        self.textrow(r,115,['BLANK INPUTS','Required numeric blanks or invalid values produce unavailable calculations and failed input checks. Fix the input; never replace an unknown with zero.'])

    def monthly_headers(self,name,title):
        self.textrow(name,1,[title])
        self.textrow(name,2,['Monthly forecast',self.currency])
        for m in range(1,37):
            c=m+2
            self.set(name,2,c,m,INT)
            self.f(name,3,c,f'EDATE({self.a("start")},{m-1})',DATE)

    def team(self):
        self.monthly_headers('Team','CODIFIX | REGIONAL TEAM AND COST ALLOCATION')
        self.textrow('Team',4,['Role','Annual base salary'])
        for c,v in [(39,'Lean hire month'),(40,'Delivery share'),(41,'Function'),(42,'Source / status'),(43,'Lean included (0 / 1)'),(44,'Original hire month'),(45,'Historical included'),(46,'Active hire month'),(47,'Active included')]:self.set('Team',4,c,v)
        delivery={7:.10,10:.10,12:.60,16:1.0}
        lean_hires={5:1,6:1,7:2,8:4,12:8,10:10,13:12,16:16}
        for r in range(5,23):
            self.set('Team',r,1,self.src[f'Team!A{r}'])
            self.set('Team',r,2,self.src[f'Team!B{r}'],MONEY,True,note=f'Legacy {self.region} Team B{r}; unvalidated salary, retained 2026-09-15. Unit: local currency/year; owner: founder/finance.')
            self.set('Team',r,39,lean_hires.get(r,self.src[f'Team!AN{r}']),INT,True,note='Approved lean schedule, 2026-09-15; month index; founder/finance owner; operating hypothesis. Excluded roles retain their original hire index for reference.')
            self.set('Team',r,40,delivery.get(r,0),PCT,True,note='Provisional delivery allocation; salary and employee AI charged once. Customer success fully direct, solutions 60%, platform/enterprise 10%. Owner founder/finance; share; 2026-09-15.')
            group='GTM' if r in [11,13,17,18,21] else 'Product'
            self.set('Team',r,41,group)
            self.set('Team',r,42,'Legacy salary; approved lean hiring / inclusion; founder planning hypothesis, 2026-09-15')
            self.set('Team',r,43,1 if r in lean_hires else 0,INT,True,note='Approved lean role inclusion, 2026-09-15; numeric 0/1; founder/finance owner; unvalidated capacity hypothesis. Historical profile uses AS instead.')
            self.set('Team',r,44,self.src[f'Team!AN{r}'],INT,note=f'Immutable original Team AN{r} hiring assumption; regional source snapshot, 2026-09-15.')
            self.set('Team',r,45,1,INT,note='Original 18-role plan; historical reference, not current hiring authority.')
            self.f('Team',r,46,f'IF({self.a("staffing")}=1,$AR{r},IF(AND(ISNUMBER($AM{r}),$AM{r}>=1,$AM{r}=INT($AM{r})),$AM{r},NA()))',INT)
            self.f('Team',r,47,f'IF({self.a("staffing")}=1,$AS{r},IF(AND(ISNUMBER($AQ{r}),OR($AQ{r}=0,$AQ{r}=1)),$AQ{r},NA()))',INT)
            for m in range(1,37):
                c=m+2;cc=col(c)
                self.f('Team',r,c,f'IF({cc}$2>=$AT{r}+{self.a("hiredelay")},$AU{r}*$B{r}*{self.a("salarymult")}*{self.a("load")}/12,0)',MONEY)
        labels={24:'Total loaded payroll',25:'Reserved delivery payroll before cost ownership',26:'Reserved delivery capacity (hours)',27:'Paid headcount',28:'Operating expense payroll after allocation',29:'Product operating payroll',30:'GTM operating payroll',31:'Recruitment expense',32:'Reserved delivery FTE',40:'Historical schedule paid headcount (reference)',41:'Historical schedule loaded payroll (reference)',42:'Loaded payroll saving versus historical schedule'}
        for r,label in labels.items():self.set('Team',r,1,label)
        for m in range(1,37):
            c=m+2;x=col(c)
            self.f('Team',24,c,f'SUM({x}5:{x}22)',MONEY)
            self.f('Team',25,c,f'SUMPRODUCT({x}5:{x}22,$AN$5:$AN$22)',MONEY)
            self.f('Team',32,c,'+'.join(f'IF({x}$2>=$AT{r}+{self.a("hiredelay")},$AU{r}*$AN{r},0)' for r in range(5,23)),NUM)
            self.f('Team',26,c,f'{x}32*{self.a("productivehours")}',NUM)
            self.f('Team',27,c,'+'.join(f'IF({x}$2>=$AT{r}+{self.a("hiredelay")},$AU{r},0)' for r in range(5,23)),NUM)
            self.f('Team',28,c,f'{x}24-{x}25*{self.a("serviceowner")}',MONEY)
            for rr,group in [(29,'Product'),(30,'GTM')]:
                self.f('Team',rr,c,'+'.join(f'IF($AO{r}="{group}",{x}{r}*(1-$AN{r}*{self.a("serviceowner")}),0)' for r in range(5,23)),MONEY)
            self.f('Team',31,c,'+'.join(f'IF({x}$2=$AT{r}+{self.a("hiredelay")},$AU{r}*$B{r}*{self.a("salarymult")}*{self.a("recruit")}*{self.a("agency")},0)' for r in range(5,23)),MONEY)
            self.f('Team',40,c,'+'.join(f'IF({x}$2>=$AR{r}+{self.a("hiredelay")},$AS{r},0)' for r in range(5,23)),NUM)
            self.f('Team',41,c,'+'.join(f'IF({x}$2>=$AR{r}+{self.a("hiredelay")},$AS{r}*$B{r}*{self.a("salarymult")}*{self.a("load")}/12,0)' for r in range(5,23)),MONEY)
            self.f('Team',42,c,f'{x}41-{x}24',MONEY)
        self.textrow('Team',35,['Allocation basis','Salary and employee AI each allocated once. Contractors cover only demand above salaried capacity. Zero included roles generate zero employee-linked costs and capacity.'])
        self.textrow('Team',36,['Profile controls','Assumptions row 118 selects lean AM/AQ or preserved historical AR/AS. Included roles start at active hire plus the common hiring delay. Original role names and regional salary rows are retained.'])
        self.textrow('Team',37,['Lean role scope','Founder covers product direction and early sales; original Account executive 1 row is the commercial lead. Staff output, quality and customer delivery capacity remain hypotheses requiring M3 evidence.'])
        self.textrow('Team',44,['Reference savings','Payroll-only saving uses the same current regional salary/load/delay assumptions with the preserved original hiring schedule. It excludes AI, contractors and other cost offsets; the full historical case runs through the live forecast.'])

    def employee_ai(self):
        s='Employee AI';self.monthly_headers(s,'CODIFIX | EMPLOYEE AI SPENDING AND FUNDING HEADROOM')
        labels={5:'Active included employees',6:'Allowance per employee (GBP / month)',7:'Expected utilisation',8:'Subscription allocation ceiling (GBP / employee)',9:'Expected spend per employee (GBP / month)',10:'Subscription allocation per employee (GBP)',11:'Additional metered usage per employee (GBP)',12:'Unused allowance per employee (GBP)',14:'Total AI allowance (reporting currency)',15:'Expected subscription spending',16:'Expected additional metered usage spending',17:'Total expected AI spending',18:'Unused allowance — funding headroom, not expense',19:'Cumulative unused allowance',20:'Employee AI allocated to delivery cost',21:'Employee AI allocated to Product operating expense',22:'Employee AI allocated to GTM operating expense',23:'Total allocated AI spending',24:'Allocation residual',26:'Expected AI spending (GBP)',27:'Unused allowance (GBP)',28:'Total AI allowance (GBP)'}
        for r,label in labels.items():self.set(s,r,1,label)
        for m in range(1,37):
            c=m+2;x=col(c)
            f=lambda r,e,fmt=MONEY:self.f(s,r,c,e,fmt)
            f(5,f'Team!{x}27',NUM);f(6,self.a('employeeallowance'));f(7,self.a('employeeutil'),PCT);f(8,self.a('employeesubscription'))
            f(9,f'{x}6*{x}7');f(10,f'MIN({x}8,{x}9)');f(11,f'MAX(0,{x}9-{x}10)');f(12,f'MAX(0,{x}6-{x}9)')
            for r,perperson in [(14,6),(15,10),(16,11),(17,9),(18,12)]:f(r,f'{x}5*{x}{perperson}*Assumptions!$E$6')
            f(19,f'SUM($C18:{x}18)')
            f(20,f'Team!{x}32*{self.a("serviceowner")}*{x}9*Assumptions!$E$6')
            for rr,group in [(21,'Product'),(22,'GTM')]:
                f(rr,'+'.join(f'IF(AND({x}$2>=Team!$AT{r}+{self.a("hiredelay")},Team!$AO{r}="{group}"),Team!$AU{r}*(1-Team!$AN{r}*{self.a("serviceowner")})*{x}9*Assumptions!$E$6,0)' for r in range(5,23)))
            f(23,f'SUM({x}20:{x}22)');f(24,f'ABS({x}17-{x}23)',NUM)
            for rr,ref in [(26,17),(27,18),(28,14)]:f(rr,f'{x}{ref}/Assumptions!$E$6')
        self.textrow(s,31,['Spending policy','The subscription input is an allocation ceiling within expected spending, not a signed fixed commitment. Subscription-included usage is excluded from additional metered usage. Zero utilisation means zero expected spending.'])
        self.textrow(s,32,['Cost separation','Employee AI supports internal agentic engineering and business work. Customer-facing Memory inference and customer-owned agent runtimes stay on Costs. Employee AI receives no generic Opex contingency.'])
        self.textrow(s,33,['Funding policy','Unused first 24-month allowance increases the month 24 cash target alongside the following three-month operating buffer. This is earmarked cash, not an expense, accounting provision or supplier payment.'])
        self.textrow(s,34,['Evidence','GBP 5000 allowance,50% utilisation and eight staff are founder-selected hypotheses, dated 2026-09-15. They do not establish engineering productivity or equivalent delivery capacity. M3 must measure actual use and review/rework.'])

    def cohorts(self):
        self.monthly_headers('Cohorts','CODIFIX | ANNUAL CONTRACT COHORTS')
        self.cohort_rows={k:[] for k in ['new','logos','unit','arr','invoice','revenue','renewals']}
        for cohort in range(1,37):
            start=5+(cohort-1)*8
            keys=['new','logos','unit','arr','invoice','revenue','renewals']
            names=['New paid accounts','Active paid accounts','Annual commitment per retained account','Closing recurring ARR','Annual platform invoices','Platform service revenue','Renewing paid accounts']
            for off,(key,label) in enumerate(zip(keys,names)):
                r=start+off;self.cohort_rows[key].append(r)
                self.set('Cohorts',r,1,f'M{cohort:02d} · {label}')
            for m in range(1,37):
                c=m+2;x=col(c);cohortcol=col(cohort+2);age=m-cohort;renewals=max(0,age//12)
                if m<cohort:
                    for off in range(7):self.set('Cohorts',start+off,c,0,MONEY if off in (2,3,4,5) else NUM)
                    continue
                self.f('Cohorts',start,c,f'Revenue!{cohortcol}$6' if m==cohort else '0',NUM)
                self.f('Cohorts',start+1,c,f'Revenue!{cohortcol}$6*(1-{self.a("churn")})^{renewals}',NUM)
                self.f('Cohorts',start+2,c,f'Assumptions!$E$110*(1+{self.a("expansion")})^{renewals}',MONEY)
                self.f('Cohorts',start+3,c,f'{x}{start+1}*{x}{start+2}',MONEY)
                self.f('Cohorts',start+4,c,f'{x}{start+3}' if age%12==0 else '0',MONEY)
                self.f('Cohorts',start+5,c,f'{x}{start+3}/12',MONEY)
                self.f('Cohorts',start+6,c,f'{x}{start+1}' if age>0 and age%12==0 else '0',NUM)

    def cohortsum(self,key,c):return 'SUM('+','.join(f'Cohorts!{col(c)}{r}' for r in self.cohort_rows[key])+')'

    def revenue(self):
        self.monthly_headers('Revenue','CODIFIX | PILOTS, ACCOUNTS, REVENUE AND COLLECTIONS')
        labels={4:'Launch eligibility month',5:'Pilot starts (expected)',6:'New paid accounts (expected)',7:'Active pilot accounts',8:'Completed pilot accounts',10:'Active paying accounts',11:'Live independently administered deployments',12:'New pilot deployments onboarded',13:'Live collections',14:'Eligible questions including failed/refused',15:'Inference attempts including retries',16:'Initially failed or refused questions',17:'Stored GB including replicas',18:'Paid question workload',20:'New recurring ARR',21:'Closing recurring ARR',22:'Platform service revenue',23:'Pilot service revenue',24:'Onboarding service revenue',25:'Uncommitted usage service revenue',26:'Total recognised revenue',27:'Annual platform invoices',28:'Pilot invoices',29:'Onboarding invoices',30:'Usage invoices (prior month service)',31:'Total invoices',32:'Cash receipts',33:'Closing accounts receivable',34:'Closing deferred platform and pilot revenue',35:'Closing unbilled usage asset',37:'Renewing accounts',38:'Churned accounts'}
        for r,label in labels.items():self.set('Revenue',r,1,label)
        for m in range(1,37):
            c=m+2;x=col(c);p=col(c-1)
            f=lambda r,expr,fmt=MONEY:self.f('Revenue',r,c,expr,fmt)
            f(4,f'{self.a("firstpilot")}+{self.a("delay")}',INT)
            shifted=f'({x}$2-{self.a("delay")}-MAX(0,{self.a("firstpilot")}-5))'
            f(5,f'IF(AND({self.a("ready")}=1,{shifted}>=5,{shifted}<=36),INDEX(Assumptions!$E$95:$E$106,MAX(1,MIN(12,ROUNDUP({shifted}/3,0))))*IF(MOD({shifted}-1,3)=0,{self.a("phase1")},IF(MOD({shifted}-1,3)=1,{self.a("phase2")},{self.a("phase3")}))*{self.a("pilotmult")},0)',NUM)
            f(6,f'IF({x}$2>{self.a("duration")},INDEX($C$5:$AL$5,1,MAX(1,{x}$2-{self.a("duration")}))*{self.a("conversion")},0)',NUM)
            f(7,f'SUM(INDEX($C$5:$AL$5,1,MAX(1,{x}$2-{self.a("duration")}+1)):{x}5)',NUM)
            f(8,f'IF({x}$2>{self.a("duration")},INDEX($C$5:$AL$5,1,MAX(1,{x}$2-{self.a("duration")})),0)',NUM)
            f(10,self.cohortsum('logos',c),NUM)
            f(11,f'({x}10+{x}7)*{self.a("deploy")}',NUM)
            f(12,f'{x}5*{self.a("deploy")}',NUM)
            f(13,f'{x}11*{self.a("collections")}',NUM)
            paidq=[]
            for cohort,r in enumerate(self.cohort_rows['logos'],1):
                if m>=cohort:paidq.append(f'Cohorts!{x}{r}*{self.a("questions")}*(1+{self.a("usagegrowth")})^{((m-cohort)//12)}')
            f(18,'+'.join(paidq) or '0',NUM)
            f(14,f'{x}18+{x}7*{self.a("questions")}*{self.a("pilotqshare")}',NUM)
            f(15,f'{x}14*(1+{self.a("failure")}*{self.a("retries")})',NUM)
            f(16,f'{x}14*{self.a("failure")}',NUM)
            f(17,f'{x}13*{self.a("gbcollection")}*(1+{self.a("replicas")})',NUM)
            f(20,f'{x}6*Assumptions!$E$110')
            f(21,self.cohortsum('arr',c));f(22,self.cohortsum('revenue',c))
            f(23,f'{x}7*{self.a("pilotfee")}*Assumptions!$E$6/{self.a("duration")}')
            f(24,f'{x}6*{self.a("onboardingfee")}*Assumptions!$E$6')
            overages=[]
            for cohort,r in enumerate(self.cohort_rows['logos'],1):
                if m>=cohort:overages.append(f'Cohorts!{x}{r}*MAX(0,{self.a("questions")}*(1+{self.a("usagegrowth")})^{((m-cohort)//12)}-{self.a("includedq")})*{self.a("overage")}*Assumptions!$E$6')
            f(25,'+'.join(overages) or '0')
            f(26,f'SUM({x}22:{x}25)')
            f(27,self.cohortsum('invoice',c));f(28,f'{x}5*{self.a("pilotfee")}*Assumptions!$E$6');f(29,f'{x}24')
            f(30,f'{p}25' if m>1 else '0')
            f(31,f'SUM({x}27:{x}30)')
            f(32,f'IF({x}$2>{self.a("lag")},INDEX($C$31:$AL$31,1,MAX(1,{x}$2-{self.a("lag")})),0)')
            f(33,(f'{p}33+' if m>1 else '')+f'{x}31-{x}32')
            f(34,(f'{p}34+' if m>1 else '')+f'{x}27+{x}28-{x}22-{x}23')
            f(35,f'{x}25')
            f(37,self.cohortsum('renewals',c),NUM)
            f(38,(f'{p}10+' if m>1 else '')+f'{x}6-{x}10',NUM)
        self.textrow('Revenue',41,['Pending at horizon',None,'=AL7','Active pilots will complete after month 36; future conversions are not counted as current ARR.'])
        self.textrow('Revenue',42,['Shifted starts beyond horizon',None,f'=IF({self.a("ready")}=1,SUM(Assumptions!E95:E106)*{self.a("pilotmult")}-SUM(C5:AL5),SUM(Assumptions!E95:E106)*{self.a("pilotmult")} )','Unstarted plan retained; includes readiness hold and explicit launch delay.'])
        self.textrow('Revenue',43,['Recognition policy','Commitments are invoiced annually at start/renewal and earned monthly. Pilot fees earned over service months; onboarding earned on paid start; usage earned monthly and invoiced in arrears.'])

    def costs(self):
        self.monthly_headers('Costs','CODIFIX | BOTTOM-UP DELIVERY COST AND CAPACITY')
        labels={4:'Active service accounts (paid + pilots)',5:'Deployments',6:'Collections',7:'Questions',8:'Attempts including retries',9:'Stored GB including replicas',10:'Transferred GB',12:'Deployment infrastructure',13:'Gateway / PDS',14:'Identity / key infrastructure',15:'Origin and replica storage',16:'Transfer',17:'Backup and recovery reserve',18:'Recovery test infrastructure',19:'Memory inference',20:'Shared delivery infrastructure',21:'Codifix external Memory delivery costs',23:'Initial deployment, collection and security hours',24:'Origin-reviewed correction hours',25:'Administration and key operation hours',26:'Multi-party support and failed exchange hours',27:'Recovery exercise hours',28:'Codifix delivery hours demanded',29:'Allocated salaried delivery capacity',30:'Allocated salaried delivery cost',31:'Contractor shortfall hours',32:'Contractor delivery cost',33:'Codifix share of customer runtime spend',34:'Total cost of sales',35:'External cost of sales excluding employees',37:'Customer runtime spend outside Codifix',38:'Customer share of external Memory costs',39:'Employee AI allocated to delivery'}
        for r,label in labels.items():self.set('Costs',r,1,label)
        for m in range(1,37):
            c=m+2;x=col(c);f=lambda r,e,fmt=MONEY:self.f('Costs',r,c,e,fmt)
            for r,expr in [(4,f'Revenue!{x}10+Revenue!{x}7'),(5,f'Revenue!{x}11'),(6,f'Revenue!{x}13'),(7,f'Revenue!{x}14'),(8,f'Revenue!{x}15'),(9,f'Revenue!{x}17')]:f(r,expr,NUM)
            f(10,f'{x}8*{self.a("transfergb")}+Revenue!{x}12*{self.a("collections")}*{self.a("gbcollection")}*{self.a("replicas")}',NUM)
            for r,quantity,key in [(12,5,'infra'),(13,5,'gateway'),(14,5,'identity'),(15,9,'storage'),(16,10,'transferprice'),(17,9,'backupprice'),(18,5,'recoveryinfra')]:f(r,f'{x}{quantity}*{self.a(key)}*Assumptions!$E$6')
            f(19,f'{x}8*{self.a("tokens")}*{self.a("tokenprice")}/1000000*Assumptions!$E$6')
            f(20,f'{self.a("fixedinfra")}*Assumptions!$E$6')
            f(21,f'SUM({x}12:{x}20)*{self.a("directmult")}*{self.a("serviceowner")}')
            f(23,f'Revenue!{x}12*({self.a("deployhours")}+{self.a("collections")}*{self.a("collectionhours")})+Revenue!{x}5*{self.a("securityhours")}',NUM)
            f(24,f'{x}7*{self.a("correctionshare")}*{self.a("correctionhours")}',NUM)
            f(25,f'{x}5*({self.a("adminhours")}+{self.a("keyhours")})',NUM)
            f(26,f'{x}4*{self.a("supporthours")}+Revenue!{x}16*{self.a("failhours")}',NUM)
            f(27,f'{x}5*{self.a("recoveryhours")}',NUM)
            f(28,f'SUM({x}23:{x}27)*{self.a("supportmult")}*{self.a("serviceowner")}',NUM)
            f(29,f'Team!{x}26*{self.a("serviceowner")}',NUM)
            f(30,f'Team!{x}25*{self.a("serviceowner")}')
            f(31,f'MAX(0,{x}28-{x}29)',NUM)
            f(32,f'{x}31*{self.a("contractor")}*Assumptions!$E$6*{self.a("directmult")}')
            f(33,f'IF({self.a("agentowner")}=0,0,IF(ISNUMBER({self.a("agentcost")}),{x}4*{self.a("agentcost")}*Assumptions!$E$6*{self.a("agentowner")},NA()))')
            f(39,f"'Employee AI'!{x}20");f(34,f'{x}21+{x}30+{x}32+{x}33+{x}39');f(35,f'{x}21+{x}32+{x}33')
            f(37,f'IF(ISNUMBER({self.a("agentcost")}),{x}4*{self.a("agentcost")}*Assumptions!$E$6*(1-{self.a("agentowner")}),"PENDING")')
            f(38,f'SUM({x}12:{x}20)*{self.a("directmult")}*(1-{self.a("serviceowner")})')
        self.textrow('Costs',41,['No duplicate labour','Reserved delivery salary is paid once, including unused capacity. Additional contractor cost covers only excess demanded hours. Delivery allocations remain provisional.'])
        self.textrow('Costs',42,['Customer ownership','Customer share rows are excluded from Codifix P&L. They are spend references, not ROI or cash savings; PENDING runtime input is unmeasured and excluded from Codifix unless assigned and priced.'])

    def pnl(self):
        s='P&L & Cash';self.monthly_headers(s,'CODIFIX | PROFIT, WORKING CAPITAL AND CASH')
        labels={4:'Recognised revenue',5:'Cost of sales',6:'Gross profit',7:'Gross margin',9:'Operating payroll after delivery allocation',10:'New ARR commissions',11:'Recruitment',12:'Software and tools',13:'Workspace',14:'Marketing',15:'Events',16:'Legal, accounting and payroll',17:'SOC 2 programme',18:'Insurance',19:'Travel',20:'Contingency on other operating expenses',21:'Total operating expense',22:'Operating result before tax (EBITDA basis)',23:'Capital expenditure cash',24:'Customer receipts',25:'Total operating and capital cash outflows',26:'Net operating and capital cash flow',27:'Cash before financing (includes opening cash)',28:'Gross financing proceeds',29:'Financing fees',30:'Opening monthly cash',31:'Closing cash',33:'Accounts receivable',34:'Deferred platform and pilot revenue',35:'Unbilled usage asset',36:'Paying accounts',37:'Paid headcount',38:'Employee AI — Product and GTM operating expense'}
        for r,label in labels.items():self.set(s,r,1,label)
        for m in range(1,37):
            c=m+2;x=col(c);p=col(c-1);f=lambda r,e,fmt=MONEY:self.f(s,r,c,e,fmt)
            f(4,f'Revenue!{x}26');f(5,f'-Costs!{x}34');f(6,f'SUM({x}4:{x}5)');f(7,f'IF({x}4=0,"",{x}6/{x}4)',PCT)
            f(9,f'-Team!{x}28');f(10,f'-Revenue!{x}20*{self.a("commission")}');f(11,f'-Team!{x}31')
            f(12,f'-Team!{x}27*{self.a("tools")}');f(13,f'-Team!{x}27*{self.a("office")}')
            for r,k,start in [(14,'marketing',4),(15,'events',6),(19,'travel',6)]:f(r,f'IF({x}$2>={start},-{self.a(k)},0)')
            f(16,f'-{self.a("legal")}');f(17,f'IF({x}$2={self.a("soc2month")},-{self.a("soc2")},0)');f(18,f'-{self.a("insurance")}')
            f(38,f"-SUM('Employee AI'!{x}21:{x}22)");f(20,f'SUM({x}12:{x}19)*{self.a("contingency")}');f(21,f'SUM({x}9:{x}20)+{x}38');f(22,f'{x}6+{x}21')
            f(23,f'-{self.a("capex")}');f(24,f'Revenue!{x}32');f(25,f'-{x}5-{x}21-{x}23');f(26,f'{x}24-{x}25')
            f(27,f'{self.a("openingcash")}+SUM($C26:{x}26)')
            f(28,f'IF({x}$2={self.a("financemonth")},Summary!$B$23,0)');f(29,f'-{x}28*{self.a("fees")}')
            f(30,f'{p}31' if m>1 else self.a('openingcash'));f(31,f'{x}30+{x}26+{x}28+{x}29')
            for r,ref in [(33,33),(34,34),(35,35)]:f(r,f'Revenue!{x}{ref}')
            f(36,f'Revenue!{x}10',NUM);f(37,f'Team!{x}27',NUM)
        self.textrow(s,40,['Cash convention','Invoices settle after the chosen whole-month delay. Delivery costs and operating expenses are paid in the service month. No bad debt, VAT/sales tax, corporation tax, debt or distributions are modelled.'])
        self.textrow(s,41,['Financing convention','Gross financing is derived from 24-month cash need, months 25–27 operating cash buffer and unused first 24-month employee AI allowance. AI headroom remains cash. Arrival after an earlier deficit is explicitly infeasible; no retroactive financing.'])

    def summary(self):
        s='Summary';self.textrow(s,1,['CODIFIX | AGENTIC ENTERPRISE FINANCIAL MODEL'])
        self.textrow(s,2,[('London' if self.region=='london' else 'New York')+' company base',self.currency,'Same global product, two initial deployments per account; alternative locations must not be added together.'])
        self.textrow(s,3,['Active case','=Assumptions!B3','Provisional economics; M3 validates costs/packaging, M5 supplies customer evidence.'])
        self.textrow(s,4,['Formula version',VERSION])
        for year in range(3):
            c=year+3;startcol=col(3+year*12);endcol=col(14+year*12)
            self.f(s,5,c,f'TEXT(EDATE(Assumptions!$E$10,{year*12}),"mmm-yy")&" to "&TEXT(EDATE(Assumptions!$E$10,{year*12+11}),"mmm-yy")')
            for rr,sheet,sourcerow,flow in [(6,'Revenue',21,False),(7,'Revenue',10,False),(8,'Revenue',11,False),(9,'Revenue',26,True),(10,'P&L & Cash',6,True),(12,'Costs',34,True),(13,'P&L & Cash',21,True),(14,'P&L & Cash',22,True),(15,'Revenue',32,True),(16,'P&L & Cash',25,True),(17,'P&L & Cash',31,False),(18,'Team',27,False),(19,'Employee AI',17,True)]:
                expr=f"SUM('{sheet}'!{startcol}{sourcerow}:{endcol}{sourcerow})" if flow else f"'{sheet}'!{endcol}{sourcerow}"
                self.f(s,rr,c,expr,NUM if rr in (7,8,18) else MONEY)
            x=col(c);self.f(s,11,c,f'IF({x}9=0,"",{x}10/{x}9)',PCT)
        labels={6:'Ending recurring ARR',7:'Ending paying accounts (expected)',8:'Ending live deployments including pilots',9:'Recognised revenue',10:'Gross profit',11:'Gross margin on recognised revenue',12:'Cost of sales',13:'Operating expense',14:'Operating result before tax',15:'Customer cash receipts',16:'Operating and capital cash outflows',17:'Ending funded cash',18:'Ending paid headcount',19:'Employee AI expected spending',21:'FUNDING | 24 MONTHS + OPERATING BUFFER + AI HEADROOM',22:'Required net financing',23:'Required gross financing',24:'Financing fees',25:'Months 25–27 cash-spend buffer',26:'Minimum funded cash through month 24',27:'Closing cash at month 24',28:'Closing cash at month 36',29:'Historical net raise reference',30:'Required net financing less historical reference',31:'Months with a cash deficit before financing',32:'Required gross financing (GBP comparison)',33:'Derived annual net revenue retention',34:'USES OF CASH | FIRST 24 MONTHS',35:'Product operating payroll',36:'GTM operating payroll',37:'Salaried delivery payroll',38:'External delivery including contractors',39:'Recruitment and new ARR commissions',40:'Other operating expenses',41:'Capital expenditure',42:'Total cash uses',43:'Customer collections',44:'Opening unrestricted cash',45:'Net financing received through month 24',46:'Closing cash at month 24',49:'SENSITIVITY OUTPUTS | FORECAST TOTALS',50:'Total recognised revenue, 36 months',51:'Gross margin, 36-month revenue weighted',52:'Ending recurring ARR in GBP',53:'Active pilots pending after horizon',54:'Planned pilot starts shifted past horizon',55:'Unbilled usage asset at horizon',56:'Accounts receivable at horizon',57:'Deferred revenue at horizon'}
        for r,label in labels.items():self.set(s,r,1,label)
        f=lambda r,e,fmt=MONEY:self.f(s,r,2,e,fmt)
        f(22,'MAX(0,-MIN(\'P&L & Cash\'!C27:Z27),B25+B74-\'P&L & Cash\'!Z27)')
        f(23,f'B22/(1-{self.a("fees")})');f(24,'B23-B22');f(25,"SUM('P&L & Cash'!AA25:AC25)")
        f(26,"MIN('P&L & Cash'!C31:Z31)");f(27,"'P&L & Cash'!Z31");f(28,"'P&L & Cash'!AL31")
        f(29,self.a('legacyraise'));f(30,'B22-B29')
        f(31,f'COUNTIFS(\'P&L & Cash\'!C2:AL2,"<"&{self.a("financemonth")},\'P&L & Cash\'!C27:AL27,"<0")',INT)
        f(32,'B23/Assumptions!E6');f(33,'Assumptions!E109',PCT)
        for r,e in [(35,'SUM(Team!C29:Z29)'),(36,'SUM(Team!C30:Z30)'),(37,'SUM(Costs!C30:Z30)'),(38,'SUM(Costs!C35:Z35)'),(39,"-SUM('P&L & Cash'!C10:Z11)"),(40,"-SUM('P&L & Cash'!C12:Z20)"),(41,"-SUM('P&L & Cash'!C23:Z23)"),(42,'SUM(B35:B41)+B47'),(43,'SUM(Revenue!C32:Z32)'),(44,self.a('openingcash')),(45,"SUM('P&L & Cash'!C28:Z29)"),(46,"'P&L & Cash'!Z31"),(50,'SUM(C9:E9)'),(51,'IF(B50=0,"",SUM(C10:E10)/B50)'),(52,'E6/Assumptions!E6'),(53,'Revenue!C41'),(54,'Revenue!C42'),(55,'Revenue!AL35'),(56,'Revenue!AL33'),(57,'Revenue!AL34')]:f(r,e,PCT if r==51 else NUM if r in (53,54) else MONEY)
        self.textrow(s,47,['Employee AI expected spending (all allocations)',None]);f(47,"SUM('Employee AI'!C17:Z17)")
        for r in list(range(35,42))+[47]:self.f(s,r,3,f'IF($B$42=0,"",B{r}/$B$42)',PCT)
        self.textrow(s,71,['EMPLOYEE AI | SPENDING, RETAINED CASH AND STAFFING'])
        ai_labels={72:'Total AI allowance — first 24-months',73:'Expected AI spending — first 24-months',74:'Earmarked unused AI allowance — first 24-months',75:'Total AI allowance — full 36 months',76:'Expected AI spending — full 36 months',77:'Unused AI allowance — full 36 months (disclosure)',78:'Month24 closing cash excluding AI earmark',79:'Payroll-only saving versus historical team — 24m',80:'Payroll-only saving versus historical team — 36m',81:'Expected AI spending 24m — GBP comparison',82:'AI headroom 24m — GBP comparison',83:'Payroll-only saving 24m — GBP comparison',84:'Required month 24 cash: operating buffer + AI'}
        for rr,label in ai_labels.items():self.set(s,rr,1,label)
        for rr,expr in [(72,"SUM('Employee AI'!C14:Z14)"),(73,"SUM('Employee AI'!C17:Z17)"),(74,"SUM('Employee AI'!C18:Z18)"),(75,"SUM('Employee AI'!C14:AL14)"),(76,"SUM('Employee AI'!C17:AL17)"),(77,"SUM('Employee AI'!C18:AL18)"),(78,'B27-B74'),(79,'SUM(Team!C42:Z42)'),(80,'SUM(Team!C42:AL42)'),(81,'B73/Assumptions!E6'),(82,'B74/Assumptions!E6'),(83,'B79/Assumptions!E6'),(84,'B25+B74')]:f(rr,expr)
        self.textrow(s,86,['AI policy','Expected expense is allowance × utilisation. Subscriptions use MIN(allocation ceiling, expected spend); the remainder is incremental metered usage. Funding headroom is cash, never duplicate cost.'])
        self.textrow(s,87,['Staffing comparison','Payroll savings compare against the original 18-role hiring schedule at current salary/load inputs. They exclude AI, contractor and overhead effects; use the verified historical-staffing sensitivity for the full financial comparison.'])
        self.textrow(s,88,['Capacity evidence','Seven/eight/eight year-end staff and AI spending remain operating hypotheses. The revenue forecast is conditional on readiness; M3 must validate throughput, review/rework, actual employee AI spending and delivery capacity.'])
        self.textrow(s,60,['Interpretation','Staff capacity has value only with separate customer evidence. These company forecasts do not claim customer ROI, validated pricing, readiness, or signed business.'])
        self.textrow(s,61,['Funding timing','=IF(B31=0,"No deficit before scheduled financing in this scenario","INFEASIBLE: cash deficit precedes financing; revise opening cash or arrival month")'])
        self.textrow(s,62,['Cash presentation','Uses are gross cash spending; financing need is net of customer receipts and opening cash. Financing fees are shown separately from operating uses.'])

    def checks(self):
        s='Checks';self.monthly_headers(s,'CODIFIX | TERMINAL RECONCILIATIONS AND INPUT VALIDITY')
        self.textrow(s,4,['Check','Absolute residual / count'])
        numeric_inputs=[x for x in self.editable if x!='Assumptions!B3']
        self.set(s,5,1,'Invalid numeric input count (all cases)')
        self.f(s,5,2,f'SUM(C100:C{99+len(numeric_inputs)})',NUM)
        for i,address in enumerate(numeric_inputs,100):
            sheet,a1=address.split('!');ref=quoted(sheet,a1)
            r=int(''.join(x for x in a1 if x.isdigit()))
            constraints=[f'ISNUMBER({ref})',f'{ref}>=0']
            if sheet=='Assumptions':
                key=next(k for k,v in self.assumptions.items() if v==r)
                if key in ['churn','conversion','failure','serviceowner','agentowner','agency','phase1','phase2','phase3','correctionshare','pilotqshare','employeeutil']:constraints.append(f'{ref}<=1')
                if key=='fees':constraints.append(f'{ref}<1')
                if key in ['fx','duration','productivehours','load']:constraints.append(f'{ref}>0')
                if key in ['duration','lag','delay','financemonth','hiredelay','firstpilot','includeddeploy','deploy','collections','replicas','soc2month']:constraints.append(f'{ref}=INT({ref})')
                if key in ['duration','financemonth','firstpilot','soc2month']:constraints.append(f'{ref}<=36')
                if key in ['financemonth','firstpilot']:constraints.append(f'{ref}>=1')
                if key=='firstpilot':constraints.append(f'{ref}>=5')
                if key in ['ready','staffing']:constraints.append(f'OR({ref}=0,{ref}=1)')
                if key in ['extraopeningAR','extraopeningdeferred']:constraints.append(f'{ref}=0')
            if sheet=='Team':
                if a1.startswith('AN'):constraints.append(f'{ref}<=1')
                if a1.startswith('AM'):constraints.extend([f'{ref}=INT({ref})',f'{ref}>=1'])
                if a1.startswith('AQ'):constraints.append(f'OR({ref}=0,{ref}=1)')
            self.set(s,i,1,address)
            self.f(s,i,2,ref,NUM)
            if sheet=='Assumptions' and r==63:
                scenario_col=''.join(x for x in a1 if x.isalpha())
                expr=f'IFERROR(IF(OR(AND(ISNUMBER({ref}),{ref}>=0),AND({ref}="PENDING",Assumptions!{scenario_col}64=0)),0,1),1)'
            else:expr='IFERROR(IF(AND('+','.join(constraints)+'),0,1),1)'
            self.f(s,i,3,expr,INT)
        labels={6:'Quarterly phasing sum less one',7:'Payroll allocation residual',8:'Monthly cash bridge residual',9:'Receivables bridge residual',10:'Deferred revenue bridge residual',11:'Accrual revenue to collections bridge residual',12:'24-month uses versus cash outflows residual',13:'24-month sources and uses residual',14:'Annual commitment recognition residual',15:'Derived annual retention residual',16:'Minimum deferred or receivable balance breach',17:'Customer runtime ownership with unknown cost',18:'Invalid selected case',19:'Deployment count below included package count (allowed)',20:'Funding timing breach count (warning)',21:'Funded cash floor breach through24 (warning)',22:'Month24 cash below operating buffer plus AI headroom (warning)'}
        for r,label in labels.items():self.set(s,r,1,label)
        self.f(s,6,2,'ABS(SUM(Assumptions!B88:B90)-1)+ABS(SUM(Assumptions!C88:C90)-1)+ABS(SUM(Assumptions!D88:D90)-1)',NUM)
        for m in range(1,37):
            c=m+2;x=col(c);p=col(c-1)
            self.f(s,7,c,f'ABS(Team!{x}24-Team!{x}28-Costs!{x}30)',NUM)
            self.f(s,8,c,f'ABS(\'P&L & Cash\'!{x}31-\'P&L & Cash\'!{x}30-\'P&L & Cash\'!{x}26-\'P&L & Cash\'!{x}28-\'P&L & Cash\'!{x}29)',NUM)
            self.f(s,9,c,f'ABS(Revenue!{x}33-'+(f'Revenue!{p}33-' if m>1 else '')+f'Revenue!{x}31+Revenue!{x}32)',NUM)
            self.f(s,10,c,f'ABS(Revenue!{x}34-'+(f'Revenue!{p}34-' if m>1 else '')+f'Revenue!{x}27-Revenue!{x}28+Revenue!{x}22+Revenue!{x}23)',NUM)
            self.f(s,11,c,f'ABS(SUM(Revenue!$C26:{x}26)-SUM(Revenue!$C32:{x}32)-Revenue!{x}33+Revenue!{x}34-Revenue!{x}35)',NUM)
            self.f(s,14,c,f'ABS(Revenue!{x}21/12-Revenue!{x}22)',NUM)
            self.f(s,16,c,f'MAX(0,-Revenue!{x}33,-Revenue!{x}34,-Revenue!{x}35)',NUM)
        for r in [7,8,9,10,11,14,16]:self.f(s,r,2,f'SUM(C{r}:AL{r})','0.000000')
        self.f(s,12,2,"ABS(Summary!B42-SUM('P&L & Cash'!C25:Z25))",NUM)
        self.f(s,13,2,'ABS(Summary!B44+Summary!B43+Summary!B45-Summary!B42-Summary!B46)',NUM)
        self.f(s,15,2,f'ABS(Summary!B33-(1-{self.a("churn")})*(1+{self.a("expansion")}))','0.000000')
        self.f(s,17,2,'IF(AND(Assumptions!E64>0,NOT(ISNUMBER(Assumptions!E63))),1,0)',INT)
        self.f(s,18,2,'IF(OR(Assumptions!B3="Base",Assumptions!B3="Downside",Assumptions!B3="Upside"),0,1)',INT)
        self.f(s,19,2,'MAX(0,Assumptions!E13-Assumptions!E14)',NUM)
        self.f(s,20,2,'Summary!B31',INT)
        self.f(s,21,2,'MAX(0,-Summary!B26)',NUM)
        self.f(s,22,2,'MAX(0,Summary!B84-Summary!B27)',NUM)
        for rr,label in {28:'Employee AI allocation residual',29:'Employee AI allowance identity residual',30:'Employee AI subscription and metered spend residual',31:'Excluded role payroll residual',32:'Funding formula residual including AI headroom',33:'Month24 cash earmark presentation residual',34:'Employee AI Opex and delivery bridge residual'}.items():self.set(s,rr,1,label)
        for m in range(1,37):
            c=m+2;x=col(c)
            self.f(s,28,c,f"'Employee AI'!{x}24",NUM)
            self.f(s,29,c,f"ABS('Employee AI'!{x}14-'Employee AI'!{x}17-'Employee AI'!{x}18)",NUM)
            self.f(s,30,c,f"ABS('Employee AI'!{x}17-'Employee AI'!{x}15-'Employee AI'!{x}16)",NUM)
            self.f(s,31,c,'+'.join(f'IF(Team!$AU{r}=0,ABS(Team!{x}{r}),0)' for r in range(5,23)),NUM)
            self.f(s,34,c,f"ABS('Employee AI'!{x}17-Costs!{x}39+'P&L & Cash'!{x}38)",NUM)
        for rr in [28,29,30,31,34]:self.f(s,rr,2,f'SUM(C{rr}:AL{rr})','0.000000')
        self.f(s,32,2,"ABS(Summary!B22-MAX(0,-MIN('P&L & Cash'!C27:Z27),Summary!B25+Summary!B74-'P&L & Cash'!Z27))",NUM)
        self.f(s,33,2,'ABS(Summary!B27-Summary!B74-Summary!B78)',NUM)
        self.textrow(s,24,['Warning policy','Rows19–22 inform decisions and may be nonzero in a stress. They never drive or suppress forecast results. Reconciliation rows 5–18 and 28–34 must pass.'])
        self.textrow(s,25,['Tolerance','Residual tolerance 0.00001 in reporting currency; no rounding in forecast calculations. Opening receivables/deferred balances are constrained to zero in this version.'])
        self.textrow(s,26,['Unknown runtime cost','If Codifix is assigned any customer runtime costs, populate a measured numeric cost assumption. PENDING reference spend is unmeasured, not free.'])

    def documentation(self):
        s='README'
        content=[
            ['CODIFIX | M2 MODEL GUIDE'],
            ['Purpose','London and New York are alternative company bases for the same global Agentic Enterprise product and workload. Do not aggregate them.'],
            ['Status','Financial preparation, 2026-09-15. All prices, customer volumes, conversion, retention, salaries and delivery rates remain unvalidated planning hypotheses.'],
            ['Start here','Select Base, Downside or Upside in Assumptions B3. Edit blue inputs; inspect Summary, then Checks. Monthly forecasts run for36months from the actual date input.'],
            ['Navigation','Assumptions → Team / Employee AI / Cohorts / Revenue / Costs → P&L & Cash → Summary. Checks are terminal. Sensitivity snapshots use this one live engine.'],
            ['Commercial scope','One paying enterprise account begins with two independently administered deployments. Existing customer agent runtimes remain separate. No billing replicated knowledge copies as new knowledge.'],
            ['Pilot funnel','Quarterly source volumes are provisional pilot starts; editable monthly phasing is0/40/60%. Earliest start M5, two-month operation,50% conversion. Delay shifts starts; pending pilots and shifted starts remain visible.'],
            ['Readiness','The Base readiness switch is a conditional forecasting convention. Deployment, security, recovery, evaluation and customer approval gates remain open independently.'],
            ['Revenue basis','Annual platform and additional deployment commitments billed at paid start/anniversary and recognised evenly across12 service months. Churn and commitment expansion apply only at renewal.'],
            ['Other fees','Pilot fees recognised across pilot service months; onboarding recognised on paid conversion; usage earned monthly and invoiced one month later. All three fees default0 and are excluded from recurring ARR.'],
            ['Cash basis','Invoices settle after an editable whole-month delay. Receivables, deferred platform/pilot revenue and unbilled usage reconcile. Costs/payroll paid in-month. No VAT/sales tax, tax accrual, debt or dividends.'],
            ['Retention','5% annual logo churn and45% retained commitment expansion derive137.75% annual revenue retention. There is no independent140% retention assumption.'],
            ['Usage growth','Annual service workload growth is separately editable. Its provisional Base45% matches the legacy commitment expansion rate as an explicit cost stress assumption, not as a proven causal relationship.'],
            ['Costs','Bottom-up deployments, gateway/PDS, identity/key, collections, replicas, transfer, backup/recovery, inference, review, curation, administration and multi-party support. Failed/refused questions and retries remain in costs.'],
            ['Labour','Regional salaries and original hiring records are retained; the active lean profile has eight roles and editable inclusion. Delivery payroll and employee AI are each allocated once; only demand above reserved salaried hours buys contractors. Customer service ownership can reallocate payroll back to operating expense.'],
            ['Funding','Net need is the maximum of cash-floor need through M24 and M24 cash needed for M25–27 operating outflows plus unused first 24-month employee AI allowance, less available opening cash/collections through the unfinanced cash path. Gross proceeds include financing fees.'],
            ['Funding timing','An earlier deficit cannot be cured by later financing. Summary displays cash-timing infeasibility separately. Historical £3m/$4.5m net raises are reference cases, not automatic targets.'],
            ['Use of funds','First24month cash uses include delivery, product/GTM people, employee AI, recruitment/commission, other expenses and capex. AI headroom remains earmarked closing cash, not an expense. Customer collections, opening cash, net financing and closing cash reconcile independently.'],
            ['FX','GBP commercial prices and shared service costs convert using USD/GBP1.3476 (14Sep2026 planning snapshot). Regional wages/overheads remain local assumptions. Common GBP outputs aid comparison.'],
            ['Sensitivity refresh','Run the repository verified refresh routine. It captures each input, verifies formulas/version, recalculates cases on this engine, validates checks, restores inputs and only then publishes.'],
            ['Stale results','Sensitivity B3 compares captured inputs to all supported editable inputs with type/blank checks. Formula edits are verified during refresh; arbitrary formula changes between refreshes are not automatically marked stale.'],
            ['Ownership','Founder/finance owns planning inputs; M3 validates providers, costs and packaging; M4 updates the deck; M5 gathers customer evidence. No hiring/spending approval is implied.'],
            ['Model source','docs/reviews/federation-first/m2-model/build_model.py; companion manifests, refresh procedure and validation evidence in the same repository folder.'],
            ['LEGACY AUDIT | preserved, not current commercial assumptions'],
            ['Legacy London usage','175seats ×4,000operations ×12months ×£0.35/1,000 =£2,940 per account/year, versus independent £80,000 ACV. Legacy per-seat note was incorrect.'],
            ['Legacy New York usage','175seats ×100,000operations ×12months ×$0.45/1,000 =$94,500 per account/year versus independent $100,000 ACV. Operation volume was25× London without a common workload basis.'],
            ['Legacy operation definition','Undefined operations are not mapped to service questions or provider tokens. Current token/attempt/question assumptions are explicit new hypotheses.'],
            ['Legacy retention mismatch','Annual renewal used140%, monthly ARR implied about138%, and sensitivity used another approximation. Current cohorts use a single anniversary rule and one engine.'],
            ['Legacy costs and funding','Preset90%/70% gross margins and24month Opex-only use-of-funds have been replaced with bottom-up cost of sales and complete cash uses.'],
            ['Historical self-serve',f'Launch month {self.src.get("Assumptions!B31")}; new adds {self.src.get("Assumptions!B32")}; monthly add growth {self.src.get("Assumptions!B33")}; ARPU {self.src.get("Assumptions!B34")} {self.currency}; monthly churn {self.src.get("Assumptions!B35")}. Excluded from current enterprise forecast.'],
            ['Source backup',('https://docs.google.com/spreadsheets/d/1u14XLZQm8YsMyeYpLUdjerVUTZIaR4x_z7aTqm6aBnw/edit' if self.region=='london' else 'https://docs.google.com/spreadsheets/d/1ZfEpxpD-tQTY1AtSoYNgknsithA3QbxqF8h9ao6qu5U/edit')],
            ['Primary FX reference','https://www.bankofengland.co.uk/boeapps/database/Rates.asp?into=GBP&rateview=A'],
            ['Recognition reference','https://www.ifrs.org/issued-standards/list-of-standards/ifrs-15-revenue-from-contracts-with-customers/; simplified planning policy, not a statutory accounting opinion.'],
            ['Employee AI policy','GBP 5000 per included employee/month at 50% expected utilisation. GBP 250 subscription allocation ceiling within spending; additional metered usage excludes subscription-included use. Founder-selected operating hypotheses, not a productivity claim.'],
            ['Lean team','Founder M1, Rust M1, platform M2, retrieval M4, solutions M8, enterprise M10, commercial M12, support M16; 7/8/8 employees at year ends. Historical 18-role profile uses the same live forecast and AI policy.'],
            ['Evidence and M3','Validate actual employee AI spending, engineering throughput, review/rework and delivery capacity. Vendor-reported typical coding-assistant spend and token-spend ambitions do not establish equivalent output from fewer people.'],
            ['Technical boundary','Private Q&A uses Memory gateway. ATProto publication/federation and governance do not imply broader delegated execution; F2 remains future work.'],
        ]
        for r,row in enumerate(content,1):self.textrow(s,r,row)

    def sensitivity(self):
        s='Sensitivity';self.textrow(s,1,['CODIFIX | VERIFIED SCENARIO SNAPSHOTS'])
        self.textrow(s,2,['Snapshots use the live forecast. Refresh restores original inputs before publication. Never sum London and New York.'])
        self.textrow(s,3,['Snapshot status'])
        self.textrow(s,4,['Captured at','NOT REFRESHED'])
        self.textrow(s,5,['Input signature','NOT REFRESHED'])
        self.textrow(s,6,['Verified formula fingerprint','NOT REFRESHED'])
        headers=['Case','Captured UTC','Captured case overrides','ARR M36','Revenue36m','Gross margin36m','Gross funding','Min cash M1–24','Cash M36','Accounts M36','Staff M36','Cash spend24m','Funding GBP','ARR M36 GBP','Model version','Status','Notes','AI spend24m','AI headroom 24m','AI spend36m','Payroll saving 24m','Payroll saving36m','AI spend24m GBP','AI headroom 24m GBP','Staff M12']
        self.textrow(s,7,headers)
        self.set(s,7,27,'Captured input address');self.set(s,7,28,'Captured raw input');self.set(s,7,29,'Matches live input')
        self.capture_cells={}
        for r,address in enumerate(self.editable,8):
            sheet,a1=address.split('!');live=quoted(sheet,a1);captured=f'AB{r}'
            self.set(s,r,27,address);self.capture_cells[address]=f'Sensitivity!{captured}'
            self.f(s,r,29,f'AND(ISBLANK({live})=ISBLANK({captured}),TYPE({live})=TYPE({captured}),{live}={captured})')
        end=7+len(self.editable)
        self.f(s,3,2,f'IF(B4="NOT REFRESHED","NOT REFRESHED",IF(AND(COUNTIF(AC8:AC{end},FALSE)=0,O8=Assumptions!B4),"CURRENT INPUTS / FORMULAS VERIFIED AT REFRESH","STALE — RUN VERIFIED REFRESH"))')
        self.textrow(s,43,['Formula assurance','Input edits are detected live. The refresh checks every expected formula at start/end. After manual formula editing, restore the generated formula or rebuild and revalidate before using results.'])
        self.textrow(s,44,['Snapshot evidence','Rows8–40 are published only after original inputs are restored and formula/invariant checks pass. Concurrent spreadsheet edits can abort the best-effort refresh; Google Sheets has no transaction lock.'])

    def manifest(self):
        return {
            'modelVersion':VERSION,'region':self.region,'currency':self.currency,
            'spreadsheetId':self.legacy['spreadsheetId'],'sheetIds':self.ids,
            'editableCells':self.editable,'formulaCells':self.formulas,
            'scenarioCell':'Assumptions!B3',
            'inputLabels':{address:self.input_label(address) for address in self.editable},
            'immutableCells':self.immutable_cells(),
            'controlCells':self.editable,'baseControls':{'Assumptions!B3':'Base'},
            'nullableOutputKeys':['grossMargin36'],
            'cases':[
                {'id':'base','label':'Base','inputs':{'Assumptions!B3':'Base'}},
                {'id':'downside','label':'Downside','inputs':{'Assumptions!B3':'Downside'}},
                {'id':'upside','label':'Upside','inputs':{'Assumptions!B3':'Upside'}},
                {'id':'price_minus20','label':'Package price −20%','inputTransforms':{'Assumptions!B12':{'multiply':.8}}},
                {'id':'price_plus20','label':'Package price +20%','inputTransforms':{'Assumptions!B12':{'multiply':1.2}}},
                {'id':'conversion_half','label':'Pilot conversion halved','inputTransforms':{'Assumptions!B25':{'multiply':.5}}},
                {'id':'launch_delay3','label':'Launch delayed 3 months','inputTransforms':{'Assumptions!B22':{'add':3}}},
                {'id':'zero_sales','label':'Zero paid conversion','inputs':{'Assumptions!B25':0}},
                {'id':'renewal_downside','label':'10% churn / 15% expansion','inputs':{'Assumptions!B26':.10,'Assumptions!B27':.15}},
                {'id':'delivery_plus50','label':'External delivery rates +50%','inputTransforms':{'Assumptions!B66':{'multiply':1.5}}},
                {'id':'support_double','label':'Delivery effort doubled','inputTransforms':{'Assumptions!B67':{'multiply':2}}},
                {'id':'collections_delay2','label':'Collections 2 months slower','inputTransforms':{'Assumptions!B29':{'add':2}}},
                {'id':'failed_exchanges_double','label':'Initial failures doubled','inputTransforms':{'Assumptions!B35':{'multiply':2,'max':1}}},
                {'id':'fx_low','label':'USD/GBP 1.20','inputs':{'Assumptions!B11':1.2}},
                {'id':'fx_high','label':'USD/GBP 1.50','inputs':{'Assumptions!B11':1.5}},
                {'id':'extra_replica','label':'One additional stored replica','inputTransforms':{'Assumptions!B44':{'add':1}},'notes':'Storage and transfer costs change; no extra knowledge charge.'},
                {'id':'readiness_hold','label':'Readiness gate remains closed','inputs':{'Assumptions!B23':0},'notes':'Staff/fixed costs continue; pilot plan remains pending.'},
                {'id':'ai_util25','label':'Employee AI utilisation 25%','inputs':{'Assumptions!B120':.25}},
                {'id':'ai_util50','label':'Employee AI utilisation 50%','inputs':{'Assumptions!B120':.50}},
                {'id':'ai_util100','label':'Employee AI utilisation 100%','inputs':{'Assumptions!B120':1}},
                {'id':'ai_budget2000','label':'AI allowance GBP 2000 per employee','inputs':{'Assumptions!B119':2000}},
                {'id':'ai_budget5000','label':'AI allowance GBP 5000 per employee','inputs':{'Assumptions!B119':5000}},
                {'id':'ai_budget10000','label':'AI allowance GBP 10000 per employee','inputs':{'Assumptions!B119':10000}},
                {'id':'historical_team_ai','label':'Historical 18-role schedule, same AI policy','inputs':{'Assumptions!B118':1},'notes':'Same live engine, commercial assumptions and employee AI policy. Historical salary rows and hire dates retained.'},
                {'id':'lean_capacity_stress','label':'Launch delay 3 / support 2× / AI 100%','inputs':{'Assumptions!B120':1},'inputTransforms':{'Assumptions!B22':{'add':3},'Assumptions!B67':{'multiply':2}},'notes':'Conditional target stress; AI spending does not prove lean-team productivity.'},
            ],
            'outputCells':{'arr36':'Summary!E6','accounts36':'Summary!E7','deployments36':'Summary!E8','revenue36':'Summary!B50','grossMargin36':'Summary!B51','requiredGrossFunding':'Summary!B23','minCash24':'Summary!B26','endingCash36':'Summary!B28','staff36':'Summary!E18','totalSpend24':'Summary!B42','gbpFunding':'Summary!B32','gbpARR36':'Summary!B52','aiSpend24':'Summary!B73','aiReserve24':'Summary!B74','aiSpend36':'Summary!B76','payrollSavings24':'Summary!B79','payrollSavings36':'Summary!B80','gbpAISpend24':'Summary!B81','gbpAIReserve24':'Summary!B82','staff12':'Summary!C18'},
            'checks':[{'cell':f'Checks!B{r}','expected':0,'tolerance':.00001,'label':self.cells['Checks'][(r,1)]['userEnteredValue']['stringValue']} for r in list(range(5,19))+list(range(28,35))],
            'publication':{'sheet':'Sensitivity','resultStartRow':8,'resultEndRow':40,'captureCells':self.capture_cells,'capturedAtCell':'Sensitivity!B4','signatureCell':'Sensitivity!B5','formulaFingerprintCell':'Sensitivity!B6'},
            'assumptionRows':self.assumptions,
            'initialInputValues':{a:self.raw_value(a) for a in self.editable},
            'modelNotes':['Figures are unvalidated planning hypotheses.','Separate annual commitment expansion and service workload growth assumptions both default45%.','No customer ROI or readiness conclusion.','Employee AI allowance GBP 5000 at 50% utilisation and lean eight-role team are founder-selected operating hypotheses.','Unused first 24-month AI allowance is earmarked cash in funding target; no generic Opex contingency on employee AI.'],
        }

    def immutable_cells(self):
        editable=set(self.editable)
        dynamic={'Sensitivity!B4','Sensitivity!B5','Sensitivity!B6'}
        result={}
        for sheet,cells in self.cells.items():
            for (r,c),cell in cells.items():
                address=f'{sheet}!{col(c)}{r}';value=cell.get('userEnteredValue',{})
                if address not in editable and address not in dynamic and value and 'formulaValue' not in value:
                    result[address]=next(iter(value.values()))
        return result

    def input_label(self,address):
        sheet,a1=address.split('!');r=int(''.join(x for x in a1 if x.isdigit()))
        if address=='Assumptions!B3':return 'Selected case'
        label=self.cells[sheet].get((r,1),{}).get('userEnteredValue',{}).get('stringValue',address)
        if sheet=='Assumptions':
            unit=self.cells[sheet].get((r,6),{}).get('userEnteredValue',{}).get('stringValue','')
            return f'{label} ({unit})'
        kind='annual salary' if a1.startswith('B') else 'lean hire month' if a1.startswith('AM') else 'lean included (0 / 1)' if a1.startswith('AQ') else 'delivery allocation'
        return f'{label} {kind}'

    def raw_value(self,address):
        sheet,a1=address.split('!');r=int(''.join(x for x in a1 if x.isdigit()));letters=''.join(x for x in a1 if x.isalpha());c=0
        for ch in letters:c=c*26+ord(ch)-64
        return next(iter(self.cells[sheet][(r,c)]['userEnteredValue'].values()))

    def requests(self):
        req=[]
        names=['README','Summary','Assumptions','Team','Employee AI','Revenue','Costs','Cohorts','P&L & Cash','Sensitivity','Checks']
        for index,name in enumerate(names):
            sheetid=self.ids[name]
            if name not in self.original_ids:req.append({'addSheet':{'properties':{'sheetId':sheetid,'title':name,'gridProperties':{'rowCount':1000,'columnCount':47}}}})
            req.append({'updateSheetProperties':{'properties':{'sheetId':sheetid,'index':index,'gridProperties':{'rowCount':1000,'columnCount':47,'frozenRowCount':8 if name=='Assumptions' else 7 if name=='Sensitivity' else 4 if name in ('Team','Checks') else 3 if name in ('Revenue','Costs','Cohorts','P&L & Cash','Employee AI') else 5 if name=='Summary' else 1,'frozenColumnCount':0,'hideGridlines':True}},'fields':'index,gridProperties'}})
            # Rebuild content in observed native tabs; backups retain historical cells/formatting.
            req.append({'unmergeCells':{'range':{'sheetId':sheetid,'startRowIndex':0,'endRowIndex':1000,'startColumnIndex':0,'endColumnIndex':47}}})
            req.append({'updateCells':{'range':{'sheetId':sheetid,'startRowIndex':0,'endRowIndex':1000,'startColumnIndex':0,'endColumnIndex':47},'fields':'userEnteredValue,userEnteredFormat,note,dataValidation'}})
            req.append({'repeatCell':{'range':{'sheetId':sheetid,'startRowIndex':0,'endRowIndex':1000,'startColumnIndex':0,'endColumnIndex':47},'cell':{'userEnteredFormat':{'backgroundColor':WHITE,'textFormat':{'fontFamily':'Arial','fontSize':10,'foregroundColor':INK},'verticalAlignment':'MIDDLE'}},'fields':'userEnteredFormat'}})
            vals=self.cells[name];maxrow=max(r for r,c in vals);maxcol=max(c for r,c in vals)
            rows=[]
            for r in range(1,maxrow+1):
                rows.append({'values':[vals.get((r,c),{}) for c in range(1,maxcol+1)]})
            req.append({'updateCells':{'start':{'sheetId':sheetid,'rowIndex':0,'columnIndex':0},'rows':rows,'fields':'userEnteredValue,userEnteredFormat.textFormat.foregroundColor,userEnteredFormat.numberFormat,note'}})
            req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'ROWS','startIndex':0,'endIndex':maxrow},'properties':{'pixelSize':27},'fields':'pixelSize'}})
            req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'COLUMNS','startIndex':0,'endIndex':1},'properties':{'pixelSize':355 if name!='Cohorts' else 365},'fields':'pixelSize'}})
            req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'COLUMNS','startIndex':1,'endIndex':47},'properties':{'pixelSize':132},'fields':'pixelSize'}})
            if name=='README':
                req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'COLUMNS','startIndex':0,'endIndex':1},'properties':{'pixelSize':205},'fields':'pixelSize'}})
                req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'COLUMNS','startIndex':1,'endIndex':2},'properties':{'pixelSize':970},'fields':'pixelSize'}})
                req.append({'repeatCell':{'range':{'sheetId':sheetid,'startRowIndex':1,'endRowIndex':maxrow,'startColumnIndex':1,'endColumnIndex':2},'cell':{'userEnteredFormat':{'wrapStrategy':'WRAP'}},'fields':'userEnteredFormat.wrapStrategy'}})
                req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'ROWS','startIndex':1,'endIndex':maxrow},'properties':{'pixelSize':56},'fields':'pixelSize'}})
            if name=='Assumptions':
                for start,end,width in [(0,1,370),(5,6,175),(6,7,500),(7,8,110),(8,9,150),(9,10,260)]:req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'COLUMNS','startIndex':start,'endIndex':end},'properties':{'pixelSize':width},'fields':'pixelSize'}})
                req.append({'repeatCell':{'range':{'sheetId':sheetid,'startRowIndex':8,'endRowIndex':123,'startColumnIndex':5,'endColumnIndex':10},'cell':{'userEnteredFormat':{'wrapStrategy':'WRAP','textFormat':{'fontSize':9,'foregroundColor':GRAY}}},'fields':'userEnteredFormat.wrapStrategy,userEnteredFormat.textFormat'}})
                req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'ROWS','startIndex':8,'endIndex':123},'properties':{'pixelSize':43},'fields':'pixelSize'}})
                for rr in (2,7,112,113,114,115,123):
                    if rr==2:range_={'sheetId':sheetid,'startRowIndex':rr-1,'endRowIndex':rr,'startColumnIndex':0,'endColumnIndex':10}
                    else:range_={'sheetId':sheetid,'startRowIndex':rr-1,'endRowIndex':rr,'startColumnIndex':1,'endColumnIndex':10}
                    req.append({'mergeCells':{'range':range_,'mergeType':'MERGE_ALL'}})
                    req.append({'repeatCell':{'range':range_,'cell':{'userEnteredFormat':{'wrapStrategy':'WRAP'}},'fields':'userEnteredFormat.wrapStrategy'}})
            if name=='Sensitivity':
                req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'COLUMNS','startIndex':2,'endIndex':3},'properties':{'pixelSize':350},'fields':'pixelSize'}})
                req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'COLUMNS','startIndex':26,'endIndex':29},'properties':{'hiddenByUser':True},'fields':'hiddenByUser'}})
                for rr in [2,43,44]:
                    start=0 if rr==2 else 1
                    ran={'sheetId':sheetid,'startRowIndex':rr-1,'endRowIndex':rr,'startColumnIndex':start,'endColumnIndex':10}
                    req.append({'mergeCells':{'range':ran,'mergeType':'MERGE_ALL'}})
                    req.append({'repeatCell':{'range':ran,'cell':{'userEnteredFormat':{'wrapStrategy':'WRAP'}},'fields':'userEnteredFormat.wrapStrategy'}})
            # Notes span readable native cells without obscuring figures.
            note_rows={'Summary':[60,61,62,66,86,87,88],'Team':[35,36,37,44],'Employee AI':[31,32,33,34],'Revenue':[43],'Costs':[41,42],'P&L & Cash':[40,41],'Checks':[24,25,26]}.get(name,[])
            for rr in note_rows:
                ran={'sheetId':sheetid,'startRowIndex':rr-1,'endRowIndex':rr,'startColumnIndex':1,'endColumnIndex':6 if name=='Summary' else 12}
                req.append({'mergeCells':{'range':ran,'mergeType':'MERGE_ALL'}})
                req.append({'repeatCell':{'range':ran,'cell':{'userEnteredFormat':{'wrapStrategy':'WRAP'}},'fields':'userEnteredFormat.wrapStrategy'}})
                req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'ROWS','startIndex':rr-1,'endIndex':rr},'properties':{'pixelSize':58},'fields':'pixelSize'}})
            if name=='Summary':
                for rr in [2,3]:
                    ran={'sheetId':sheetid,'startRowIndex':rr-1,'endRowIndex':rr,'startColumnIndex':2,'endColumnIndex':6}
                    req.append({'mergeCells':{'range':ran,'mergeType':'MERGE_ALL'}})
                    req.append({'repeatCell':{'range':ran,'cell':{'userEnteredFormat':{'wrapStrategy':'WRAP'}},'fields':'userEnteredFormat.wrapStrategy'}})
                    req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'ROWS','startIndex':rr-1,'endIndex':rr},'properties':{'pixelSize':45},'fields':'pixelSize'}})
                for rr in [21,34,49,71]:req.append({'mergeCells':{'range':{'sheetId':sheetid,'startRowIndex':rr-1,'endRowIndex':rr,'startColumnIndex':0,'endColumnIndex':6},'mergeType':'MERGE_ALL'}})
            if name=='Revenue':
                for rr in [41,42]:req.append({'mergeCells':{'range':{'sheetId':sheetid,'startRowIndex':rr-1,'endRowIndex':rr,'startColumnIndex':3,'endColumnIndex':12},'mergeType':'MERGE_ALL'}})
            if name=='Sensitivity':
                for rr in [3,4,5,6]:req.append({'mergeCells':{'range':{'sheetId':sheetid,'startRowIndex':rr-1,'endRowIndex':rr,'startColumnIndex':1,'endColumnIndex':17},'mergeType':'MERGE_ALL'}})
            # Main banner and actual header rows only.
            bannerend=2 if name=='README' else 10 if name=='Assumptions' else 17 if name=='Sensitivity' else 6 if name=='Summary' else 12
            req.append({'mergeCells':{'range':{'sheetId':sheetid,'startRowIndex':0,'endRowIndex':1,'startColumnIndex':0,'endColumnIndex':bannerend},'mergeType':'MERGE_ALL'}})
            req.append({'repeatCell':{'range':{'sheetId':sheetid,'startRowIndex':0,'endRowIndex':1,'startColumnIndex':0,'endColumnIndex':bannerend},'cell':{'userEnteredFormat':{'backgroundColor':NAVY,'textFormat':{'foregroundColor':WHITE,'bold':True,'fontSize':15}}},'fields':'userEnteredFormat.backgroundColor,userEnteredFormat.textFormat'}})
            req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'ROWS','startIndex':0,'endIndex':1},'properties':{'pixelSize':42},'fields':'pixelSize'}})
            hdr=8 if name=='Assumptions' else 7 if name=='Sensitivity' else 5 if name=='Summary' else 3 if name not in ('README',) else None
            if hdr:req.append({'updateDimensionProperties':{'range':{'sheetId':sheetid,'dimension':'ROWS','startIndex':hdr-1,'endIndex':hdr},'properties':{'pixelSize':45},'fields':'pixelSize'}})
            if hdr:req.append({'repeatCell':{'range':{'sheetId':sheetid,'startRowIndex':hdr-1,'endRowIndex':hdr,'startColumnIndex':0,'endColumnIndex':10 if name=='Assumptions' else 25 if name=='Sensitivity' else 6 if name=='Summary' else 38},'cell':{'userEnteredFormat':{'backgroundColor':TEAL,'textFormat':{'foregroundColor':WHITE,'bold':True},'wrapStrategy':'WRAP'}},'fields':'userEnteredFormat.backgroundColor,userEnteredFormat.textFormat,userEnteredFormat.wrapStrategy'}})
            totals={'Summary':[6,9,10,17,21,22,23,25,34,42,46,47,49,71,73,74,76,78,84],'Revenue':[21,26,31,32,33,34],'Costs':[21,28,30,34,35],'Team':[24,25,27,28,40,41,42],'Employee AI':[14,17,18,20,21,22,23,26,27,28],'P&L & Cash':[4,6,21,22,25,27,31],'Checks':[5,12,13,20,21,22],'Assumptions':[91,109,110,118,119,120,121]}.get(name,[])
            for r in totals:
                req.append({'repeatCell':{'range':{'sheetId':sheetid,'startRowIndex':r-1,'endRowIndex':r,'startColumnIndex':0,'endColumnIndex':6 if name=='Summary' else 10 if name=='Assumptions' else 38},'cell':{'userEnteredFormat':{'backgroundColor':PALE,'textFormat':{'bold':True},'borders':{'top':{'style':'SOLID','color':{'red':.75,'green':.83,'blue':.86}}}}},'fields':'userEnteredFormat.backgroundColor,userEnteredFormat.textFormat.bold,userEnteredFormat.borders'}})
        # Native input validation: required numeric scenario values and exact case dropdown.
        req.append({'setDataValidation':{'range':{'sheetId':self.ids['Assumptions'],'startRowIndex':2,'endRowIndex':3,'startColumnIndex':1,'endColumnIndex':2},'rule':{'condition':{'type':'ONE_OF_LIST','values':[{'userEnteredValue':v} for v in ['Base','Downside','Upside']]},'strict':True,'showCustomUi':True,'inputMessage':'Select the one live forecast case.'}}})
        for r in self.assumptions.values():
            condition={'type':'CUSTOM_FORMULA','values':[{'userEnteredValue':'=OR(ISNUMBER(B63),B63="PENDING")'}]} if r==63 else {'type':'NUMBER_BETWEEN','values':[{'userEnteredValue':'0'},{'userEnteredValue':'1'}]} if r in [118,120] else {'type':'NUMBER_GREATER_THAN_EQ','values':[{'userEnteredValue':'0'}]}
            req.append({'setDataValidation':{'range':{'sheetId':self.ids['Assumptions'],'startRowIndex':r-1,'endRowIndex':r,'startColumnIndex':1,'endColumnIndex':4},'rule':{'condition':condition,'strict':True,'inputMessage':'Required planning input. PENDING allowed only for unassigned customer runtime cost. See Checks.'}}})
        req.append({'setDataValidation':{'range':{'sheetId':self.ids['Team'],'startRowIndex':4,'endRowIndex':22,'startColumnIndex':42,'endColumnIndex':43},'rule':{'condition':{'type':'CUSTOM_FORMULA','values':[{'userEnteredValue':'=AND(ISNUMBER(AQ5),OR(AQ5=0,AQ5=1))'}]},'strict':True,'inputMessage':'Required numeric role inclusion:0 or1; used by the lean staffing profile.'}}})
        req.append({'addConditionalFormatRule':{'index':0,'rule':{'ranges':[{'sheetId':self.ids['Checks'],'startRowIndex':4,'endRowIndex':18,'startColumnIndex':1,'endColumnIndex':2}],'booleanRule':{'condition':{'type':'NUMBER_GREATER','values':[{'userEnteredValue':'0.00001'}]},'format':{'backgroundColor':{'red':1,'green':.88,'blue':.88},'textFormat':{'bold':True,'foregroundColor':{'red':.65,'green':0,'blue':0}}}}}}})
        return [r for r in req if 'addSheet' in r]+[r for r in req if 'addSheet' not in r]

    def build(self):
        self.setup();self.team();self.employee_ai();self.cohorts();self.revenue();self.costs();self.pnl();self.summary();self.checks();self.documentation();self.sensitivity()
        # Buffer expressly excludes capital cash: operating outflows only, per approved plan.
        self.f('Summary',25,2,"SUM('P&L & Cash'!AA25:AC25)+SUM('P&L & Cash'!AA23:AC23)",MONEY)
        self.textrow('Summary',65,['Funded runway (months from start)', "=IF(COUNTIF('P&L & Cash'!C31:AL31,\"<0\")=0,\">36\",MINIFS('P&L & Cash'!C2:AL2,'P&L & Cash'!C31:AL31,\"<0\")-1)"])
        self.textrow('Summary',66,['Historical net raise coverage', '=IF(B29>=B22,"Covers calculated net requirement","Below calculated net requirement")'])
        self.textrow('Summary',64,['Recalculation sentinel','=SUM(Assumptions!E10:E90)+SUM(Assumptions!E95:E106)+SUM(Team!B5:B22)+SUM(Team!AM5:AN22)+SUM(Team!AQ5:AQ22)+SUM(Assumptions!E118:E121)'])
        return self


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--region',choices=['london','newyork','both'],default='both');parser.add_argument('--output-dir',default='/tmp');args=parser.parse_args()
    out=pathlib.Path(args.output_dir);out.mkdir(parents=True,exist_ok=True)
    for region in (['london','newyork'] if args.region=='both' else [args.region]):
        model=Model(region).build();manifest=model.manifest();requests=model.requests()
        (out/f'm2-{region}-batch.json').write_text(json.dumps({'requests':requests},ensure_ascii=False))
        (out/f'm2-{region}-manifest.json').write_text(json.dumps(manifest,indent=2,ensure_ascii=False)+'\n')
        print(json.dumps({'region':region,'requests':len(requests),'formulas':len(model.formulas),'inputs':len(model.editable),'assumptions':len(model.assumptions),'batch':str(out/f'm2-{region}-batch.json'),'manifest':str(out/f'm2-{region}-manifest.json')}))

if __name__=='__main__':main()
