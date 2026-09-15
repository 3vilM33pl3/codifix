#!/usr/bin/env python3
"""Check generated native model manifests before publication; no Google API calls."""
import json
import pathlib
import re
import sys


def col_number(label):
    n=0
    for char in label:n=n*26+ord(char)-64
    return n


def col_label(n):
    result=''
    while n:
        n,r=divmod(n-1,26);result=chr(r+65)+result
    return result


def check_ai_revision(model,errors):
    """Check the approved operating basis and safe publication interfaces.

    Native scenario fixtures separately evaluate formula outputs; these checks
    catch generator changes that lose an input, historical source or control.
    """
    inputs=model['initialInputValues'];immutable=model['immutableCells'];formulas=model['formulaCells']
    required={'staffing':118,'employeeallowance':119,'employeeutil':120,'employeesubscription':121}
    if any(model['assumptionRows'].get(k)!=v for k,v in required.items()):errors.append('AI assumption interface drift')
    if model['sheetIds'].get('Employee AI')!=2040000004:errors.append('Employee AI native tab identity drift')
    for case in 'BCD':
        if [inputs[f'Assumptions!{case}{r}'] for r in range(118,122)]!=[0,5000,.5,250]:errors.append('Approved employee AI defaults drift')
    expected_hires={5:1,6:1,7:2,8:4,10:10,12:8,13:12,16:16}
    historical=[1,1,2,3,2,5,6,8,10,12,15,16,18,19,21,23,26,28]
    heads=[]
    for month in range(1,37):
        heads.append(sum(inputs[f'Team!AQ{r}'] for r in range(5,23) if month>=inputs[f'Team!AM{r}']))
    if [heads[i] for i in [11,23,35]]!=[7,8,8] or sum(heads[:24])!=146 or sum(heads)!=242:errors.append('Approved lean hiring economics drift')
    for r,old_hire in zip(range(5,23),historical):
        if inputs[f'Team!AQ{r}']!=(1 if r in expected_hires else 0):errors.append(f'Lean role inclusion drift: {r}')
        if r in expected_hires and inputs[f'Team!AM{r}']!=expected_hires[r]:errors.append(f'Lean role hire drift: {r}')
        if immutable.get(f'Team!AR{r}')!=old_hire or immutable.get(f'Team!AS{r}')!=1:errors.append(f'Historical staff source lost: {r}')
        if not all(f'Team!{c}{r}' in inputs for c in ['AM','AN','AQ']):errors.append(f'Uncaptured staffing control: {r}')
    for col in [col_label(c) for c in range(3,39)]:
        # Prevent common economic wiring mistakes, without claiming that these
        # static checks replace the independent native reconciliation fixtures.
        if f'{col}38' not in formulas[f'P&L & Cash!{col}21']:errors.append('Employee AI Opex omitted')
        if f'{col}38' in formulas[f'P&L & Cash!{col}20']:errors.append('AI incorrectly receives generic Opex contingency')
        if f'{col}39' not in formulas[f'Costs!{col}34']:errors.append('Delivery AI omitted from cost of sales')
    if 'B74' not in formulas['Summary!B22'] or '+B47' not in formulas['Summary!B42']:errors.append('AI funding or cash-spending bridge omitted')
    case_ids={x['id'] for x in model['cases']}
    if not {'ai_util25','ai_util50','ai_util100','ai_budget2000','ai_budget5000','ai_budget10000','historical_team_ai','lean_capacity_stress'}<=case_ids:errors.append('Missing approved AI sensitivities')
    publication=model['publication']
    if len(model['cases'])>publication['resultEndRow']-publication['resultStartRow']+1:errors.append('Sensitivity result range too short')
    if any(not re.fullmatch(r'Sensitivity!AB\d+',x) for x in publication['captureCells'].values()):errors.append('Captured values moved into comparison outputs')
    if not all(f'Checks!B{r}' in {x['cell'] for x in model['checks']} for r in range(28,35)):errors.append('Mandatory AI reconciliations omitted')


def run(london_path,newyork_path):
    london=json.loads(pathlib.Path(london_path).read_text())
    newyork=json.loads(pathlib.Path(newyork_path).read_text())
    errors=[]
    if london['formulaCells']!=newyork['formulaCells']:errors.append('Regional formula structures differ')
    for model in [london,newyork]:
        check_ai_revision(model,errors)
        inputs=model['editableCells'];formula=model['formulaCells']
        if len(inputs)!=len(set(inputs)):errors.append('Duplicate editable input')
        if set(inputs)!=set(model['publication']['captureCells']):errors.append('Incomplete captured-input coverage')
        if model['baseControls']!={'Assumptions!B3':'Base'}:errors.append('Base controls overwrite current economic inputs')
        if not all(f'Assumptions!{c}{r}' in model['immutableCells'] for r in [86,87] for c in 'BCD'):errors.append('Unsupported opening balances not immutable')
        for address,text in formula.items():
            stripped=re.sub(r'"(?:[^"]|"")*"','""',text)
            level=0
            for ch in stripped:
                if ch=='(':level+=1
                if ch==')':level-=1
                if level<0:break
            if level!=0:errors.append(f'Parentheses: {address}')
            if re.search(r'\b(LET|LAMBDA|MAP|ARRAYFORMULA|TABLE)\s*\(',stripped,re.I):errors.append(f'Prohibited formula pattern: {address}')
        sheet_names='|'.join(re.escape(x) for x in model['sheetIds'])
        pattern=re.compile(rf"(?:(?:'([^']+)'|({sheet_names}))!)?(\$?[A-Z]{{1,3}}\$?\d+)(?::(\$?[A-Z]{{1,3}}\$?\d+))?")
        graph={}
        for address,text in formula.items():
            sheet=address.split('!')[0];refs=[]
            for match in pattern.finditer(re.sub(r'"(?:[^"]|"")*"','""',text)):
                refsheet=match[1] or match[2] or sheet
                start=match[3].replace('$','');end=(match[4] or match[3]).replace('$','')
                first=re.fullmatch(r'([A-Z]+)(\d+)',start);last=re.fullmatch(r'([A-Z]+)(\d+)',end)
                for ri in range(int(first[2]),int(last[2])+1):
                    for ci in range(col_number(first[1]),col_number(last[1])+1):
                        target=f'{refsheet}!{col_label(ci)}{ri}'
                        if target in formula:refs.append(target)
            graph[address]=refs
        state={}
        def visit(node):
            if state.get(node)==1:raise ValueError(f'Formula dependency cycle: {node}')
            if state.get(node)==2:return
            state[node]=1
            for target in graph[node]:visit(target)
            state[node]=2
        try:
            for node in graph:visit(node)
        except ValueError as exc:errors.append(str(exc))
    result={'passed':not errors,'checks':['identical regional formula structure','all editable inputs uniquely captured','sparse case controls preserve live baseline','fixed opening balances immutable','approved lean headcounts and employee-month totals','historical staff sources immutable','employee AI cash and allocation wiring','AI sensitivity and capture interfaces','scalar formula syntax and balanced parentheses','no formula dependency cycles'],'formulas_per_region':len(london['formulaCells']),'inputs_per_region':len(london['editableCells']),'errors':errors}
    print(json.dumps(result,indent=2));return 0 if not errors else 1

if __name__=='__main__':
    sys.exit(run(*(sys.argv[1:] or ['/tmp/m2-london-manifest.json','/tmp/m2-newyork-manifest.json'])))
