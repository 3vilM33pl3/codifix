import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useProjectData } from './data';
import { canWrite, errorText, projectPath, request, time, type AuthMeResponse, type MemoryEntryResponse, type ReplacementProposalListResponse, type ReplacementProposalResolutionResponse } from './service';
import { Badge, Empty, Intro, QueryState, Technical } from './ui';
export function Review({scope, project, auth, connected, onBack, notify}: {scope:string; project:string; auth:AuthMeResponse; connected:boolean; onBack:()=>void; notify:(message:string)=>void}) {
  const proposals=useProjectData<ReplacementProposalListResponse>(scope,project,'replacement-proposals');
  const [selected,setSelected]=useState('');
  const [receipt,setReceipt]=useState('');
  const client=useQueryClient();
  const items=proposals.data?.proposals??[];
  const proposal=items.find(p=>p.id===selected)??items[0];
  const current=useQuery({queryKey:[scope,project,'memory',proposal?.target_memory_id],queryFn:()=>request<MemoryEntryResponse>(`/v1/memory/${proposal!.target_memory_id}`),enabled:!!proposal,retry:false});
  const decision=useMutation({mutationFn:async (action:'approve'|'reject')=>{
    if(!proposal)throw new Error('No proposal selected.');
    const result=await request<ReplacementProposalResolutionResponse>(projectPath(project,`replacement-proposals/${proposal.id}/${action}`),null);
    const expected=action==='approve'?'approved':'rejected';
    if(result.project!==project || result.proposal_id!==proposal.id || result.status!==expected)throw new Error('The review receipt did not match. Refresh before making another decision.');
    return expected;
  },onSuccess:async result=>{setReceipt(`Update ${result}. The source and replacement history remain available.`);notify(`Knowledge update ${result}`);await client.invalidateQueries({queryKey:[scope,project]});}});
  const locked=!connected || !canWrite(auth,project,'memory_curate') || !!proposals.error || !!current.error || !current.data || current.data.project!==project || decision.isPending;
  return <><Intro title="Review knowledge updates" subtitle="Compare the proposed change with what your project already knows."/>{receipt && <div role="status" className="notice positive">{receipt}</div>}<QueryState query={proposals}>{proposal ? <><div className="row"><label>Update <select value={proposal.id} disabled={decision.isPending} onChange={e=>{setSelected(e.target.value);decision.reset();setReceipt('');}}>{items.map(p=><option key={p.id} value={p.id}>{p.candidate_summary}</option>)}</select></label><Badge tone="warning">{items.length} to review</Badge></div><div className="compare"><section className="card"><span className="eyebrow">CURRENT</span><h2>{proposal.target_summary}</h2><QueryState query={current}><p className="prose">{current.data?.project===project ? current.data.canonical_text : ''}</p></QueryState><p className="caption">{time(current.data?.updated_at)}</p></section><section className="card accent"><span className="eyebrow">PROPOSED</span><h2>{proposal.candidate_summary}</h2><p className="prose">{proposal.candidate_canonical_text}</p><strong>Why this change?</strong><ul>{proposal.reasons.map((reason,i)=><li key={i}>{reason}</li>)}</ul></section></div><section className="card"><strong>Evidence stays attached</strong><p className="muted">The original source and replacement history remain available after approval.</p><Technical label="Source and proposal details" value={{sources:current.data?.sources,proposal}}/></section>{decision.error && <p role="alert" className="notice danger">Review outcome unconfirmed. {errorText(decision.error)} Refresh the queue before continuing.</p>}<div className="row"><button className="ghost" onClick={onBack}>Back to Home</button><div className="actions"><button disabled={locked} onClick={()=>decision.mutate('reject')}>Reject update</button><button className="primary" disabled={locked} onClick={()=>decision.mutate('approve')}>{decision.isPending?'Saving decision…':'Approve update'}</button></div></div>{!canWrite(auth,project,'memory_curate')&&<p className="caption">Reviewing updates requires memory curation permission on this project.</p>}</> : <Empty title="No updates waiting for review">New proposals will appear here as your agents capture knowledge.</Empty>}</QueryState></>;
}
