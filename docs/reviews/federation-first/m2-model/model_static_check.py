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


def run(london_path,newyork_path):
    london=json.loads(pathlib.Path(london_path).read_text())
    newyork=json.loads(pathlib.Path(newyork_path).read_text())
    errors=[]
    if london['formulaCells']!=newyork['formulaCells']:errors.append('Regional formula structures differ')
    for model in [london,newyork]:
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
    result={'passed':not errors,'checks':['identical regional formula structure','all editable inputs uniquely captured','sparse case controls preserve live baseline','fixed opening balances immutable','scalar formula syntax and balanced parentheses','no formula dependency cycles'],'formulas_per_region':len(london['formulaCells']),'inputs_per_region':len(london['editableCells']),'errors':errors}
    print(json.dumps(result,indent=2));return 0 if not errors else 1

if __name__=='__main__':
    sys.exit(run(*(sys.argv[1:] or ['/tmp/m2-london-manifest.json','/tmp/m2-newyork-manifest.json'])))
