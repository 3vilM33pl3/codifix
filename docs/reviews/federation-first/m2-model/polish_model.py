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
    fmt('Assumptions',117,118,1,5,'0')
    fmt('Team',4,22,42,47,'0')
    fmt('Team',45,46,42,47,'0')
    fmt('Assumptions',127,128,1,5,'0')
    for start,end in [(3,5),(6,9),(11,14),(17,24)]: fmt('Sensitivity',7,40,start,end,money)
    fmt('Sensitivity',7,40,5,6,'0.0%;(0.0%);"–"')
    fmt('Sensitivity',7,40,9,10,'0.00')
    fmt('Sensitivity',7,40,10,11,'0')
    fmt('Sensitivity',7,40,24,25,'0')
    for start,end,width in [(0,1,255),(1,2,180),(2,3,350)]:
        req.append({'updateDimensionProperties': {'range': {'sheetId': ids['Sensitivity'], 'dimension': 'COLUMNS', 'startIndex': start, 'endIndex': end}, 'properties': {'pixelSize': width}, 'fields': 'pixelSize'}})
    req.append({'repeatCell': {'range': {'sheetId':ids['Sensitivity'],'startRowIndex':6,'endRowIndex':40,'startColumnIndex':0,'endColumnIndex':25},'cell':{'userEnteredFormat':{'wrapStrategy':'WRAP'}},'fields':'userEnteredFormat.wrapStrategy'}})
    req.append({'updateDimensionProperties': {'range': {'sheetId':ids['Sensitivity'],'dimension':'ROWS','startIndex':6,'endIndex':40},'properties':{'pixelSize':46},'fields':'pixelSize'}})
    teal={'red':0,'green':.42,'blue':.42}; white={'red':1,'green':1,'blue':1}
    # Apply after new-tab cell writes: formula/reference colours must not obscure headers.
    for r,end in [(1,12),(3,38)]:
        req.append({'repeatCell':{'range':{'sheetId':ids['Employee AI'],'startRowIndex':r-1,'endRowIndex':r,'startColumnIndex':0,'endColumnIndex':end},'cell':{'userEnteredFormat':{'textFormat':{'foregroundColor':white,'bold':True,'fontSize':15 if r==1 else 10}}},'fields':'userEnteredFormat.textFormat'}})
    for sheet,r,c0,c1 in [('Summary',71,0,6),('Sensitivity',7,17,25),('Team',4,38,47)]:
        ran={'sheetId':ids[sheet],'startRowIndex':r-1,'endRowIndex':r,'startColumnIndex':c0,'endColumnIndex':c1}
        req.append({'repeatCell':{'range':ran,'cell':{'userEnteredFormat':{'backgroundColor':teal,'textFormat':{'foregroundColor':white,'bold':True},'wrapStrategy':'WRAP'}},'fields':'userEnteredFormat.backgroundColor,userEnteredFormat.textFormat,userEnteredFormat.wrapStrategy'}})
        if sheet=='Summary':req.append({'mergeCells':{'range':ran,'mergeType':'MERGE_ALL'}})
    for sheet,rows,end in [('Summary',[70,86,87,88],6),('Team',[36,37,44,48],12),('Assumptions',[123,130],10),('Sensitivity',[43,44],10)]:
        for r in rows:
            ran={'sheetId':ids[sheet],'startRowIndex':r-1,'endRowIndex':r,'startColumnIndex':1,'endColumnIndex':end}
            req.append({'mergeCells':{'range':ran,'mergeType':'MERGE_ALL'}})
            req.append({'repeatCell':{'range':ran,'cell':{'userEnteredFormat':{'wrapStrategy':'WRAP'}},'fields':'userEnteredFormat.wrapStrategy'}})
            req.append({'updateDimensionProperties':{'range':{'sheetId':ids[sheet],'dimension':'ROWS','startIndex':r-1,'endIndex':r},'properties':{'pixelSize':62},'fields':'pixelSize'}})
    for sheet,r0,r1,c0,c1 in [('Summary',71,84,0,2),('Summary',57,59,0,2),('Assumptions',117,121,0,10),('Assumptions',125,128,0,10),('Employee AI',4,29,0,1),('Team',3,4,38,47),('Team',45,46,0,47),('Checks',27,36,0,2)]:
        req.append({'repeatCell':{'range':{'sheetId':ids[sheet],'startRowIndex':r0,'endRowIndex':r1,'startColumnIndex':c0,'endColumnIndex':c1},'cell':{'userEnteredFormat':{'wrapStrategy':'WRAP'}},'fields':'userEnteredFormat.wrapStrategy'}})
        req.append({'updateDimensionProperties':{'range':{'sheetId':ids[sheet],'dimension':'ROWS','startIndex':r0,'endIndex':r1},'properties':{'pixelSize':48},'fields':'pixelSize'}})
    req.append({'updateDimensionProperties':{'range':{'sheetId':ids['Employee AI'],'dimension':'COLUMNS','startIndex':0,'endIndex':1},'properties':{'pixelSize':450},'fields':'pixelSize'}})
    req.append({'updateDimensionProperties':{'range':{'sheetId':ids['Team'],'dimension':'COLUMNS','startIndex':42,'endIndex':47},'properties':{'pixelSize':145},'fields':'pixelSize'}})
    req.append({'updateDimensionProperties':{'range':{'sheetId':ids['Team'],'dimension':'COLUMNS','startIndex':41,'endIndex':42},'properties':{'pixelSize':420},'fields':'pixelSize'}})
    # Case notes qualify the financial counterfactuals; show them in full.
    req.append({'autoResizeDimensions':{'dimensions':{'sheetId':ids['Sensitivity'],'dimension':'ROWS','startIndex':7,'endIndex':40}}})
    req.append({'repeatCell':{'range':{'sheetId':ids['Checks'],'startRowIndex':18,'endRowIndex':22,'startColumnIndex':0,'endColumnIndex':2},'cell':{'userEnteredFormat':{'wrapStrategy':'WRAP'}},'fields':'userEnteredFormat.wrapStrategy'}})
    req.append({'updateDimensionProperties':{'range':{'sheetId':ids['Checks'],'dimension':'ROWS','startIndex':18,'endIndex':22},'properties':{'pixelSize':58},'fields':'pixelSize'}})
    for i,(chart_id,sheet,row,title) in enumerate([
        (CHARTS[region][0],'Revenue',21,'Recurring ARR'),
        (CHARTS[region][1],'P&L & Cash',31,'Closing cash after modelled financing')]):
        def source(r): return {'sourceRange': {'sources': [{'sheetId':ids[sheet],'startRowIndex':r-1,'endRowIndex':r,'startColumnIndex':2,'endColumnIndex':38}]}}
        spec={'title':f'{title} ({manifest["currency"]})','subtitle':'Selected live scenario · unvalidated planning assumptions','fontName':'Arial','basicChart':{'chartType':'LINE','legendPosition':'NO_LEGEND','headerCount':0,'axis':[{'position':'BOTTOM_AXIS','title':'Month'},{'position':'LEFT_AXIS','title':manifest['currency']}],'domains':[{'domain':source(3)}],'series':[{'series':source(row),'targetAxis':'LEFT_AXIS','colorStyle':{'rgbColor':{'red':0.0,'green':0.42,'blue':0.42}}}]}}
        req.append({'updateChartSpec':{'chartId':chart_id,'spec':spec}})
        req.append({'updateEmbeddedObjectPosition':{'objectId':chart_id,'newPosition':{'overlayPosition':{'anchorCell':{'sheetId':ids['Summary'],'rowIndex':90,'columnIndex':0},'offsetXPixels':i*615,'offsetYPixels':0,'widthPixels':605,'heightPixels':340}},'fields':'*'}})
    return {'requests':req}

if __name__ == '__main__':
    for region in CHARTS:
        m=json.loads(Path(f'/tmp/m2-{region}-manifest.json').read_text())
        path=Path(f'/tmp/m2-{region}-polish.json');path.write_text(json.dumps(build(region,m)))
        print(path)
