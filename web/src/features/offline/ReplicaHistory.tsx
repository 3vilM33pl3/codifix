import { useEffect, useRef, useState } from "react";
import { laptopCacheHistory, laptopCacheList } from "../../api";
import { sameCacheScope, type LaptopCacheProject, type LaptopCacheHistoryResponse } from "./types";
import { ReplicaCorrection } from "./ReplicaCorrection";

export function ReplicaHistory({ canonical, project, owner, projectId, sourceMemoryId }: { canonical: string; project?: LaptopCacheProject; owner?: string; projectId?: string; sourceMemoryId?:string }) {
  const [history, setHistory] = useState<LaptopCacheHistoryResponse | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const active = useRef<AbortController | null>(null);
  const generation = useRef(0);
  useEffect(() => {
    generation.current++; active.current?.abort(); setHistory(null); setError(""); setBusy(false);
    return () => { generation.current++; active.current?.abort(); };
  }, [canonical, project?.subscription_id, project?.device_id, project?.company_id, project?.area, project?.owner_instance_id, project?.project_id, project?.space_id, owner, projectId]);
  async function load(next = false) {
    active.current?.abort();
    const controller = new AbortController(); active.current = controller;
    const current = ++generation.current;
    setBusy(true); setError("");
    try {
      let selected = project;
      if (!selected) {
        const inventory = await laptopCacheList(controller.signal);
        const matches = inventory.projects.filter(p => p.owner_instance_id === owner && p.project_id === projectId);
        if (matches.length !== 1) throw new Error("Exact owner subscription is unavailable or ambiguous. Select it in Synchronized knowledge.");
        selected = matches[0];
      }
      const value = await laptopCacheHistory(selected.subscription_id, canonical, next ? history?.next_after_version ?? 0 : 0, controller.signal);
      if (controller.signal.aborted || current !== generation.current) return;
      if (!sameCacheScope(selected, value.project)) throw new Error("History scope changed; refresh the selected subscription.");
      if (next && history && !sameCacheScope(history.project, value.project)) throw new Error("History subscription changed during pagination.");
      setHistory(next && history ? { ...value, versions: [...history.versions, ...value.versions] } : value);
    } catch (e) {
      if (!controller.signal.aborted && current === generation.current) { setHistory(null); setError(e instanceof Error ? e.message : "Owner history unavailable."); }
    } finally { if (!controller.signal.aborted && current === generation.current) setBusy(false); }
  }
  return <section aria-label="Read-only owner history">
    <button type="button" disabled={busy} onClick={() => void load()}>View owner history</button>
    {busy && <p role="status">Reading downloaded owner history…</p>}
    {error && <p role="alert">{error}</p>}
    {history && <>
      <p>Downloaded owner history · read-only. Canonical {history.canonical_id}; owner {history.project.owner_instance_id}; cursor {history.project.cursor}.</p>
      {!history.versions.length && <p>No retained downloaded versions for this identity.</p>}
      {history.versions.map(v => <article key={v.id}>
        <h5>Version {v.version}{v.tombstone ? " · tombstone" : ""}: {v.summary}</h5>
        <p style={{ whiteSpace: "pre-wrap" }}>{v.text}</p>
        <p>Author {v.author}; authored {v.authored_at}; record {v.id}; digest {v.digest}.</p>
        {!!v.unavailable_source_ids.length && <p>Unavailable source references: {v.unavailable_source_ids.join(", ")}. Source bytes remain local to the owner.</p>}
        {v.id===sourceMemoryId&&!v.tombstone&&history.current_state?.status==="active"&&<ReplicaCorrection key={`${v.id}/${v.digest}`} history={history} version={v} />}
      </article>)}
      {history.next_after_version !== null && <button type="button" disabled={busy} onClick={() => void load(true)}>More owner versions</button>}
    </>}
  </section>;
}
