#!/usr/bin/env python3
"""Native display/chart finish after build_model.py; no remote writes.

Chart IDs were observed in the original native workbooks on 2026-09-15.
Only formatting and chart specifications/positions change; model cells do not.
"""
import json
from pathlib import Path

CHARTS = {'london': (1555076228, 569678607), 'newyork': (860833828, 956960419)}

def build(region, manifest):
    ids = manifest['sheetIds']; req = []
    def fmt(sheet, r0, r1, c0, c1, pattern):
        req.append({'repeatCell': {'range': {'sheetId': ids[sheet], 'startRowIndex': r0, 'endRowIndex': r1, 'startColumnIndex': c0, 'endColumnIndex': c1}, 'cell': {'userEnteredFormat': {'numberFormat': {'type': 'NUMBER', 'pattern': pattern}}}, 'fields': 'userEnteredFormat.numberFormat'}})
    money = '#,##0;(#,##0);"–"'
    fmt('Summary',32,33,1,2,'0.00%')
    fmt('Summary',63,64,1,2,'0.000000')
    fmt('Assumptions',108,109,4,5,'0.00%')
    for start,end in [(3,5),(6,9),(11,14)]: fmt('Sensitivity',7,31,start,end,money)
    fmt('Sensitivity',7,31,5,6,'0.0%;(0.0%);"–"')
    fmt('Sensitivity',7,31,9,10,'0.00')
    fmt('Sensitivity',7,31,10,11,'0')
    for start,end,width in [(0,1,255),(1,2,180),(2,3,350)]:
        req.append({'updateDimensionProperties': {'range': {'sheetId': ids['Sensitivity'], 'dimension': 'COLUMNS', 'startIndex': start, 'endIndex': end}, 'properties': {'pixelSize': width}, 'fields': 'pixelSize'}})
    req.append({'repeatCell': {'range': {'sheetId':ids['Sensitivity'],'startRowIndex':6,'endRowIndex':31,'startColumnIndex':0,'endColumnIndex':17},'cell':{'userEnteredFormat':{'wrapStrategy':'WRAP'}},'fields':'userEnteredFormat.wrapStrategy'}})
    req.append({'updateDimensionProperties': {'range': {'sheetId':ids['Sensitivity'],'dimension':'ROWS','startIndex':6,'endIndex':31},'properties':{'pixelSize':46},'fields':'pixelSize'}})
    for i,(chart_id,sheet,row,title) in enumerate([
        (CHARTS[region][0],'Revenue',21,'Recurring ARR'),
        (CHARTS[region][1],'P&L & Cash',31,'Closing cash after modelled financing')]):
        def source(r): return {'sourceRange': {'sources': [{'sheetId':ids[sheet],'startRowIndex':r-1,'endRowIndex':r,'startColumnIndex':2,'endColumnIndex':38}]}}
        spec={'title':f'{title} ({manifest["currency"]})','subtitle':'Selected live scenario · unvalidated planning assumptions','fontName':'Arial','basicChart':{'chartType':'LINE','legendPosition':'NO_LEGEND','headerCount':0,'axis':[{'position':'BOTTOM_AXIS','title':'Month'},{'position':'LEFT_AXIS','title':manifest['currency']}],'domains':[{'domain':source(3)}],'series':[{'series':source(row),'targetAxis':'LEFT_AXIS','colorStyle':{'rgbColor':{'red':0.0,'green':0.42,'blue':0.42}}}]}}
        req.append({'updateChartSpec':{'chartId':chart_id,'spec':spec}})
        req.append({'updateEmbeddedObjectPosition':{'objectId':chart_id,'newPosition':{'overlayPosition':{'anchorCell':{'sheetId':ids['Summary'],'rowIndex':68,'columnIndex':0},'offsetXPixels':i*615,'offsetYPixels':0,'widthPixels':605,'heightPixels':340}},'fields':'*'}})
    return {'requests':req}

if __name__ == '__main__':
    for region in CHARTS:
        m=json.loads(Path(f'/tmp/m2-{region}-manifest.json').read_text())
        path=Path(f'/tmp/m2-{region}-polish.json');path.write_text(json.dumps(build(region,m)))
        print(path)
