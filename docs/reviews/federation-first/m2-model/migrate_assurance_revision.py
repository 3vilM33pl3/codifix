#!/usr/bin/env python3
"""Prepare a bounded v2-to-v3 delta; never write to Google Sheets directly.

Requires exact live v2 formula/immutable verification and current input captures.
Only the explicitly separated accounting/payroll assumption changes an existing
input. New counsel and lead controls are visible planning hypotheses.
"""
import json
from pathlib import Path
from build_model import Model, col


def build(region):
    old = json.loads(Path(f'/tmp/m2-assurance-v2-{region}-manifest.json').read_text())
    inputs = json.loads(Path(f'/tmp/m2-assurance-v2-{region}-inputs.json').read_text())
    if old['modelVersion'] != 'M2-2026-09-15-v2-ai':
        raise ValueError('This migration requires the verified v2 AI model')
    model = Model(region).build()
    manifest = model.manifest()
    if manifest['modelVersion'] != 'M2-2026-09-15-v3-assurance':
        raise ValueError('Unexpected target version')
    original = {**old['formulaCells'], **old['immutableCells'], **inputs}
    permitted = {f'Assumptions!{c}78' for c in 'BCD'}
    for cell in (set(inputs) & set(manifest['editableCells'])) - permitted:
        if model.raw_value(cell) != inputs[cell]:
            raise ValueError(f'Existing working input differs from generator: {cell}; preserve it before migration')
    writes = []
    current = {}
    changed = []
    for sheet, cells in model.cells.items():
        for (r, c), data in cells.items():
            address = f'{sheet}!{col(c)}{r}'
            value = data.get('userEnteredValue', {})
            if not value:
                continue
            current[address] = next(iter(value.values()))
            if original.get(address) != current[address]:
                changed.append(address)
                writes.append({'updateCells': {'start': {'sheetId': manifest['sheetIds'][sheet], 'rowIndex': r-1, 'columnIndex': c-1}, 'rows': [{'values': [data]}], 'fields': 'userEnteredValue,userEnteredFormat.textFormat.foregroundColor,userEnteredFormat.numberFormat,note'}})
    removed = []
    for address in original.keys() - current.keys():
        sheet, a1 = address.split('!')
        r = int(''.join(c for c in a1 if c.isdigit()))
        c = 0
        for letter in ''.join(c for c in a1 if c.isalpha()):
            c = c*26 + ord(letter)-64
        removed.append(address)
        writes.append({'updateCells': {'start': {'sheetId': manifest['sheetIds'][sheet], 'rowIndex': r-1, 'columnIndex': c-1}, 'rows': [{'values': [{}]}], 'fields': 'userEnteredValue,note'}})
    writes.sort(key=lambda r: tuple(r['updateCells']['start'][k] for k in ('sheetId','rowIndex','columnIndex')))
    compact = []
    for request in writes:
        body = request['updateCells']
        previous = compact[-1]['updateCells'] if compact else {}
        start, prior = body['start'], previous.get('start')
        if prior and body['fields'] == previous['fields'] and start['sheetId'] == prior['sheetId'] and start['rowIndex'] == prior['rowIndex'] and start['columnIndex'] == prior['columnIndex'] + len(previous['rows'][0]['values']):
            previous['rows'][0]['values'].extend(body['rows'][0]['values'])
        else:
            compact.append(request)
    # Prior comparisons become unavailable until a complete verified v3 refresh.
    requests = [{'updateCells': {'range': {'sheetId': manifest['sheetIds']['Sensitivity'], 'startRowIndex': 7, 'endRowIndex': 40, 'startColumnIndex': 0, 'endColumnIndex': 25}, 'fields': 'userEnteredValue'}}] + compact
    requests.extend(r for r in model.requests() if 'setDataValidation' in r)
    # Only apply the model's layout instructions for newly populated areas.
    for sheet, start, end in [('Assumptions',125,128),('Team',45,46),('Summary',67,70)]:
        requests.append({'updateDimensionProperties': {'range': {'sheetId': manifest['sheetIds'][sheet], 'dimension': 'ROWS', 'startIndex': start, 'endIndex': end}, 'properties': {'pixelSize': 58}, 'fields': 'pixelSize'}})
        requests.append({'repeatCell': {'range': {'sheetId': manifest['sheetIds'][sheet], 'startRowIndex': start, 'endRowIndex': end, 'startColumnIndex': 0, 'endColumnIndex': 10 if sheet == 'Assumptions' else 6 if sheet == 'Summary' else 47}, 'cell': {'userEnteredFormat': {'wrapStrategy': 'WRAP'}}, 'fields': 'userEnteredFormat.wrapStrategy'}})
    for sheet, row, end in [('Summary',70,6),('Assumptions',130,10)]:
        area = {'sheetId': manifest['sheetIds'][sheet], 'startRowIndex': row-1, 'endRowIndex': row, 'startColumnIndex': 1, 'endColumnIndex': end}
        requests.append({'mergeCells': {'range': area, 'mergeType': 'MERGE_ALL'}})
        requests.append({'repeatCell': {'range': area, 'cell': {'userEnteredFormat': {'wrapStrategy': 'WRAP'}}, 'fields': 'userEnteredFormat.wrapStrategy'}})
        requests.append({'updateDimensionProperties': {'range': {'sheetId': manifest['sheetIds'][sheet], 'dimension': 'ROWS', 'startIndex': row-1, 'endIndex': row}, 'properties': {'pixelSize': 80}, 'fields': 'pixelSize'}})
    for sheet, row in [('Summary',58),('Summary',59),('Employee AI',29),('Team',33),('Checks',35),('Checks',36)]:
        requests.append({'repeatCell': {'range': {'sheetId': manifest['sheetIds'][sheet], 'startRowIndex': row-1, 'endRowIndex': row, 'startColumnIndex': 0, 'endColumnIndex': 2}, 'cell': {'userEnteredFormat': {'wrapStrategy': 'WRAP'}}, 'fields': 'userEnteredFormat.wrapStrategy'}})
        requests.append({'updateDimensionProperties': {'range': {'sheetId': manifest['sheetIds'][sheet], 'dimension': 'ROWS', 'startIndex': row-1, 'endIndex': row}, 'properties': {'pixelSize': 58}, 'fields': 'pixelSize'}})
    # Numeric inclusion fields show zero explicitly, so excluded is never blank.
    for sheet, row, left, right in [('Team',46,42,47),('Assumptions',128,1,5)]:
        requests.append({'repeatCell': {'range': {'sheetId': manifest['sheetIds'][sheet], 'startRowIndex': row-1, 'endRowIndex': row, 'startColumnIndex': left, 'endColumnIndex': right}, 'cell': {'userEnteredFormat': {'numberFormat': {'type': 'NUMBER', 'pattern': '0'}}}, 'fields': 'userEnteredFormat.numberFormat'}})
    return {'requests': requests}, {'region': region, 'from': old['modelVersion'], 'to': manifest['modelVersion'], 'changedCells': changed, 'removedCells': sorted(removed), 'requestCount': len(requests), 'existingInputChanges': sorted(permitted), 'newInputs': sorted(set(manifest['editableCells'])-set(inputs))}


if __name__ == '__main__':
    for region in ('london','newyork'):
        batch, audit = build(region)
        Path(f'/tmp/m2-assurance-{region}-delta.json').write_text(json.dumps(batch))
        Path(f'/tmp/m2-assurance-{region}-delta-audit.json').write_text(json.dumps(audit, indent=2)+'\n')
        print(region, len(audit['changedCells']), 'changed;', len(audit['removedCells']), 'removed;', audit['requestCount'], 'requests')
