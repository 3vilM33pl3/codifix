/** Native Google Sheets connector adapter. No network or credential handling. */
export function createConnectorAdapter(tools, manifest, snapshotWrites) {
  const id = manifest.spreadsheetId;
  if (!id) throw new Error('A verified spreadsheetId is required');
  const sheetIds = manifest.sheetIds;
  if (!sheetIds) throw new Error('Observed sheet IDs are required');
  function address(a1) {
    const m = a1.match(/^(?:'((?:[^']|'')+)'|([^!]+))!\$?([A-Z]+)\$?([0-9]+)$/);
    if (!m) throw new Error(`Expected one qualified A1 cell: ${a1}`);
    return { sheet: (m[1] || m[2]).replaceAll("''", "'"),
      col: [...m[3]].reduce((n,c)=>n*26+c.charCodeAt(0)-64,0)-1,
      row: Number(m[4])-1 };
  }
  function letters(n) { let s=''; for(n++;n;n=Math.floor((n-1)/26)) s=String.fromCharCode(65+(n-1)%26)+s; return s; }
  function quote(s) { return "'"+s.replaceAll("'","''")+"'"; }
  function result(r) {
    if (r.isError) throw new Error(JSON.stringify(r.content));
    if (r.structuredContent) return r.structuredContent;
    for (const c of r.content || []) if (c.type==='text') {
      try { return JSON.parse(c.text); } catch {}
    }
    throw new Error('Connector did not return structured spreadsheet data');
  }
  async function readCells(cells, {mode='value'}={}) {
    const groups={};
    for (const key of cells) {
      const p=address(key); const g=groups[p.sheet] ||= {items:[],minRow:p.row,maxRow:p.row,minCol:p.col,maxCol:p.col};
      g.items.push({key,...p});g.minRow=Math.min(g.minRow,p.row);g.maxRow=Math.max(g.maxRow,p.row);
      g.minCol=Math.min(g.minCol,p.col);g.maxCol=Math.max(g.maxCol,p.col);
    }
    if (!cells.length) return {};
    const ranges=Object.entries(groups).map(([s,g])=>`${quote(s)}!${letters(g.minCol)}${g.minRow+1}:${letters(g.maxCol)}${g.maxRow+1}`);
    const data=result(await tools.mcp__codex_apps__google_drive_get_spreadsheet_cells({
      spreadsheet_id:id,ranges,cell_fields:'userEnteredValue,effectiveValue'
    }));
    const indexed={};
    for (const s of data.sheets || []) {
      const entries=indexed[s.properties.title] ||= {};
      for(const d of s.data || []) for(const [ri,row] of (d.rowData || []).entries())
        for(const [ci,cell] of (row.values || []).entries()) entries[`${(d.startRow||0)+ri},${(d.startColumn||0)+ci}`]=cell;
    }
    const out={};
    for(const g of Object.values(groups)) for(const p of g.items) {
      const cell=indexed[p.sheet]?.[`${p.row},${p.col}`] || {};
      const v=mode==='formula'?cell.userEnteredValue:cell.effectiveValue;
      if(v?.errorValue) throw new Error(`${p.key}: ${v.errorValue.type}: ${v.errorValue.message}`);
      out[p.key]=v?.formulaValue ?? v?.numberValue ?? v?.stringValue ?? v?.boolValue ?? null;
    }
    return out;
  }
  async function writeCells(values) {
    const byRow={};
    for(const [key,value] of Object.entries(values)) {
      const p=address(key);
      if(sheetIds[p.sheet]===undefined) throw new Error(`Unobserved sheet: ${p.sheet}`);
      const rows=byRow[p.sheet] ||= {}; const cols=rows[p.row] ||= {};
      cols[p.col] = value===null ? {} : {userEnteredValue:
        typeof value==='number'?{numberValue:value}:typeof value==='boolean'?{boolValue:value}:
        String(value).startsWith('=')?{formulaValue:String(value)}:{stringValue:String(value)}};
    }
    const requests=[];
    for(const [sheet,rows] of Object.entries(byRow)) {
      const blocks=[];
      for(const row of Object.keys(rows).map(Number).sort((a,b)=>a-b)) {
        const cols=Object.keys(rows[row]).map(Number).sort((a,b)=>a-b);
        let run=[];
        const flush=()=>{if(!run.length)return; const start=run[0], end=run.at(-1)+1;
          const old=blocks.find(b=>b.startColumn===start && b.endColumn===end && b.endRow===row);
          const values=run.map(c=>rows[row][c]);
          if(old){old.rows.push({values});old.endRow++;}else blocks.push({startRow:row,endRow:row+1,startColumn:start,endColumn:end,rows:[{values}]});
          run=[];};
        for(const col of cols){if(run.length && col!==run.at(-1)+1)flush();run.push(col);}flush();
      }
      for(const b of blocks)requests.push({updateCells:{range:{sheetId:sheetIds[sheet],startRowIndex:b.startRow,endRowIndex:b.endRow,startColumnIndex:b.startColumn,endColumnIndex:b.endColumn},rows:b.rows,fields:'userEnteredValue'}});
    }
    if(!requests.length)return;
    const r=await tools.mcp__codex_apps__google_drive_batch_update_spreadsheet({spreadsheet_id:id,requests});
    if(r.isError)throw new Error(JSON.stringify(r.content));
  }
  return {readCells,writeCells,publishSnapshot:async snapshot=>writeCells(snapshotWrites(snapshot,manifest))};
}
