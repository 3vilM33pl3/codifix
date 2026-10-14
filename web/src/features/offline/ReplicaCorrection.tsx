import { useState } from "react";
import { submitLaptopCorrection, type LaptopCorrectionRequest, type LaptopCorrectionReceipt } from "../../api";
import type { LaptopCacheHistoryResponse } from "./types";

export function ReplicaCorrection({history, version}: {history:LaptopCacheHistoryResponse;version:LaptopCacheHistoryResponse["versions"][number]}) {
  const [open,setOpen]=useState(false);
  const [text,setText]=useState(version.text);
  const [summary,setSummary]=useState(version.summary);
  const [importance,setImportance]=useState(history.current_state?.importance ?? 3);
  const [confidence,setConfidence]=useState(history.current_state?.confidence ?? 0);
  const [confirmed,setConfirmed]=useState(false);
  const [pending,setPending]=useState<LaptopCorrectionRequest|null>(null);
  const [receipt,setReceipt]=useState<LaptopCorrectionReceipt|null>(null);
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState("");
  const valid=text.trim().length>0&&summary.trim().length>0&&new TextEncoder().encode(text).length<=32768&&new TextEncoder().encode(summary).length<=2048&&Number.isInteger(importance)&&importance>=1&&importance<=5&&Number.isFinite(confidence)&&confidence>=0&&confidence<=1&&!/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(text+summary);
  async function submit() {
    if(!confirmed || busy || !valid)return;
    const request=pending ?? {operation_id:crypto.randomUUID(),source_memory_id:version.id,expected_digest:version.digest,content:{canonical_text:text,summary,memory_type:version.memory_type,importance,confidence}};
    setPending(request);setBusy(true);setError("");
    try {
      const value=await submitLaptopCorrection(history.project.subscription_id,request);
      if(value.owner_canonical_id!==history.canonical_id)throw new Error("Correction receipt has a different owner identity.");
      setReceipt(value);
    }catch(e){setError(e instanceof Error?e.message:"Correction staging failed; retry the same request.");}
    finally{setBusy(false);}
  }
  if(receipt)return <p role="status">Correction queued locally for owner review. Operation {receipt.operation_id}; expected owner version {receipt.expected_version}. This does not mean the owner accepted it.</p>;
  return <section aria-label={`Correction proposal for owner version ${version.version}`}>
    {!open?<button type="button" onClick={()=>setOpen(true)}>Propose a correction</button>:<>
      <p>Proposal to owner {history.project.owner_instance_id}; canonical {history.canonical_id}; version {version.version}; digest {version.digest}.</p>
      <label>Corrected memory text <textarea value={text} disabled={!!pending} onChange={e=>{setConfirmed(false);setText(e.target.value);}} /></label>
      <label>Correction summary <textarea value={summary} disabled={!!pending} onChange={e=>{setConfirmed(false);setSummary(e.target.value);}} /></label>
      <p>Text {new TextEncoder().encode(text).length} / 32768 bytes; summary {new TextEncoder().encode(summary).length} / 2048 bytes.</p>
      <label>Importance <input type="number" min={1} max={5} value={importance} disabled={!!pending} onChange={e=>{setConfirmed(false);setImportance(Number(e.target.value));}} /></label>
      <label>Confidence <input type="number" min={0} max={1} step={0.01} value={confidence} disabled={!!pending} onChange={e=>{setConfirmed(false);setConfidence(Number(e.target.value));}} /></label>
      <label><input type="checkbox" checked={confirmed} disabled={!!pending} onChange={e=>setConfirmed(e.target.checked)} />I reviewed this correction and want to queue it for synchronization and owner approval.</label>
      {pending&&<p>Operation {pending.operation_id}. Retrying sends the identical request.</p>}
      {error&&<p role="alert">{error}</p>}
      <button type="button" disabled={!confirmed||busy||!valid} onClick={()=>void submit()}>{busy?"Queuing correction…":pending?"Retry same correction":"Queue correction proposal"}</button>
      {!pending&&<button type="button" onClick={()=>{setOpen(false);setConfirmed(false);}}>Cancel correction</button>}
    </>}
  </section>;
}
