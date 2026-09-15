#!/usr/bin/env python3
"""Prepare a reviewed v1-to-v2 native delta; never submit bootstrap clears remotely.

Requires preflight v1 manifests and captured current inputs in /tmp. The caller
must verify those cells against the live workbook immediately before applying.
Only approved hire changes and new AI/staffing inputs replace existing inputs.
"""
import json
from pathlib import Path
from build_model import Model, col


def address(sheet, r, c):
    return f'{sheet}!{col(c)}{r}'


def build(region):
    old = json.loads(Path(f'/tmp/m2-ai-v1-{region}-manifest.json').read_text())
    inputs = json.loads(Path(f'/tmp/m2-ai-v1-{region}-inputs.json').read_text())
    model = Model(region).build()
    manifest = model.manifest()
    original = {**old['formulaCells'], **old['immutableCells'], **inputs}
    permitted = {f'Team!AM{r}' for r in (5, 6, 7, 8, 10, 12, 13, 16)}
    for cell in set(inputs) & set(manifest['editableCells']) - permitted:
        if model.raw_value(cell) != inputs[cell]:
            raise ValueError(f'Unapproved existing input change: {cell}')
    requests = []
    new_id = manifest['sheetIds']['Employee AI']
    # The only new tab. Existing cells outside the generated delta are untouched.
    requests.append({'addSheet': {'properties': {'sheetId': new_id, 'title': 'Employee AI', 'gridProperties': {'rowCount': 1000, 'columnCount': 47}}}})
    requests.append({'updateSheetProperties': {'properties': {'sheetId': manifest['sheetIds']['Team'], 'gridProperties': {'columnCount': 47}}, 'fields': 'gridProperties.columnCount'}})
    for sheet, rows, end in [('Sensitivity', [33, 34], 10)]:
        for r in rows:
            requests.append({'unmergeCells': {'range': {'sheetId': manifest['sheetIds'][sheet], 'startRowIndex': r-1, 'endRowIndex': r, 'startColumnIndex': 1, 'endColumnIndex': end}}})
    current = {}
    changed = []
    for sheet, cells in model.cells.items():
        for (r, c), data in cells.items():
            a = address(sheet, r, c)
            v = data.get('userEnteredValue', {})
            if not v:
                continue
            current[a] = next(iter(v.values()))
            if original.get(a) != current[a] or (sheet == 'Team' and c in (2, 39, 40)):
                changed.append(a)
                requests.append({'updateCells': {'start': {'sheetId': manifest['sheetIds'][sheet], 'rowIndex': r-1, 'columnIndex': c-1}, 'rows': [{'values': [data]}], 'fields': 'userEnteredValue,userEnteredFormat.textFormat.foregroundColor,userEnteredFormat.numberFormat,note'}})
    removed = []
    for a in original.keys() - current.keys():
        sheet, a1 = a.split('!'); letters = ''.join(x for x in a1 if x.isalpha()); r = int(''.join(x for x in a1 if x.isdigit())); c = 0
        for letter in letters: c = c*26+ord(letter)-64
        removed.append(a)
        requests.append({'updateCells': {'start': {'sheetId': manifest['sheetIds'][sheet], 'rowIndex': r-1, 'columnIndex': c-1}, 'rows': [{'values': [{}]}], 'fields': 'userEnteredValue,note'}})
    # Style the new tab using its generated layout. Never replay old-tab clears.
    new_style = []
    for req in model.requests():
        kind, body = next(iter(req.items()))
        sid = body.get('range', {}).get('sheetId', body.get('start', {}).get('sheetId', body.get('properties', {}).get('sheetId')))
        if sid == new_id and kind not in ('addSheet', 'updateCells', 'unmergeCells'):
            new_style.append(req)
        elif kind == 'setDataValidation':
            requests.append(req)
    # Invalidate prior comparison values; the verified refresh republishes v2.
    requests.append({'updateCells': {'range': {'sheetId': manifest['sheetIds']['Sensitivity'], 'startRowIndex': 7, 'endRowIndex': 40, 'startColumnIndex': 0, 'endColumnIndex': 25}, 'fields': 'userEnteredValue'}})
    requests = requests[:2] + new_style + requests[2:]
    # Compact adjacent cell mutations without broadening their scope.
    cell_writes = [r for r in requests if 'start' in r.get('updateCells', {})]
    other = [r for r in requests if 'start' not in r.get('updateCells', {})]
    cell_writes.sort(key=lambda r: (r['updateCells']['start']['sheetId'], r['updateCells']['start']['rowIndex'], r['updateCells']['start']['columnIndex']))
    compact = []
    for req in cell_writes:
        body = req.get('updateCells', {})
        previous = compact[-1].get('updateCells', {}) if compact else {}
        start, prior = body.get('start'), previous.get('start')
        if start and prior and body['fields'] == previous['fields'] and start['sheetId'] == prior['sheetId'] and start['rowIndex'] == prior['rowIndex'] and start['columnIndex'] == prior['columnIndex'] + len(previous['rows'][0]['values']):
            previous['rows'][0]['values'].extend(body['rows'][0]['values'])
        else:
            compact.append(req)
    compact = other + compact
    return {'requests': compact}, {'region': region, 'from': old['modelVersion'], 'to': manifest['modelVersion'], 'changedCells': changed, 'removedCells': sorted(removed), 'requestCount': len(compact), 'existingInputsPreservedExcept': sorted(permitted)}


if __name__ == '__main__':
    for region in ('london', 'newyork'):
        batch, audit = build(region)
        Path(f'/tmp/m2-ai-{region}-delta.json').write_text(json.dumps(batch))
        Path(f'/tmp/m2-ai-{region}-delta-audit.json').write_text(json.dumps(audit, indent=2)+'\n')
        print(region, len(audit['changedCells']), 'changed cells;', len(audit['removedCells']), 'cleared cells;', audit['requestCount'], 'requests')
