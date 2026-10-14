import { useEffect, useRef, useState } from "react";
import { laptopCacheList, laptopCacheSearch } from "../../api";
import { sameCacheScope, type LaptopCacheList, type LaptopCacheSearchResponse } from "./types";

export function OfflineTab() {
  const [inventory, setInventory] = useState<LaptopCacheList | null>(null);
  const [selected, setSelected] = useState("");
  const [question, setQuestion] = useState("");
  const [limit, setLimit] = useState(8);
  const [result, setResult] = useState<LaptopCacheSearchResponse | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [revision, setRevision] = useState(0);
  const search = useRef<AbortController | null>(null);
  const generation = useRef(0);
  function resetSearch() { generation.current++; search.current?.abort(); setResult(null); setError(""); setBusy(false); }
  useEffect(() => {
    const controller = new AbortController();
    generation.current++; search.current?.abort(); setResult(null); setInventory(null); setSelected(""); setQuestion(""); setError(""); setBusy(false);
    laptopCacheList(controller.signal).then(value => {
      if (!controller.signal.aborted) { setInventory(value); setSelected(value.projects[0]?.subscription_id ?? ""); }
    }).catch(e => { if (!controller.signal.aborted) setError(e instanceof Error ? e.message : "Synchronized cache unavailable or access denied."); });
    return () => { controller.abort(); search.current?.abort(); generation.current++; };
  }, [revision]);
  const project = inventory?.projects.find(p => p.subscription_id === selected);
  const bytes = new TextEncoder().encode(question).length;
  async function submit() {
    if (busy || !project || !project.initialized || !question.trim() || bytes > 4096) return;
    resetSearch(); setBusy(true);
    const controller = new AbortController(); search.current = controller;
    const version = generation.current;
    try {
      const value = await laptopCacheSearch(project.subscription_id, question, limit, controller.signal);
      if (controller.signal.aborted || version !== generation.current) return;
      if (!sameCacheScope(project, value.project)) throw new Error("Cache scope changed. Refresh the inventory before searching.");
      setResult(value);
    } catch (e) {
      if (!controller.signal.aborted && version === generation.current) setError(e instanceof Error ? e.message : "Cache search unavailable.");
    } finally { if (!controller.signal.aborted && version === generation.current) setBusy(false); }
  }
  const observed = result?.project ?? project;
  return <section aria-label="Synchronized knowledge">
    <h2>Synchronized knowledge</h2>
    <p>Read-only, untrusted shared text from this laptop’s exact configured subscription. Local deterministic search only: no remote search or model forwarding.</p>
    <p>Offline copies may be stale. Observed synchronization is not current remote authority; revocation cannot recall previously delivered bytes.</p>
    <button onClick={() => { resetSearch(); setRevision(v => v + 1); }}>Refresh synchronized projects</button>
    {error ? <p role="alert">{error}</p> : null}
    {!inventory && !error ? <p role="status">Loading local cache inventory…</p> : null}
    {inventory ? <>
      <p>Local area {inventory.area}; instance {inventory.instance_id}.</p>
      {inventory.projects.length ? <label>Cache subscription <select value={selected} onChange={e => { resetSearch(); setSelected(e.target.value); setQuestion(""); }}>
        {inventory.projects.map(p => <option key={p.subscription_id} value={p.subscription_id}>{p.area} / {p.project_id} / owner {p.owner_instance_id} / subscription {p.subscription_id}</option>)}
      </select></label> : <p>No configured synchronized projects. This does not establish remote permission.</p>}
    </> : null}
    {project && observed ? <>
      <p>Company {project.company_id}; area {project.area}; owner {project.owner_instance_id}; project {project.project_id}; space {project.space_id}; device {project.device_id}; subscription {project.subscription_id}.</p>
      <p>Last observed sync: {observed.observed_at ?? "unknown"}; verified local cursor: {Number.isSafeInteger(observed.cursor) ? observed.cursor : "large cursor (numeric precision unavailable)"}. {observed.initialized ? "Cache initialized; freshness is not guaranteed." : "Cache not initialized; no searchable data confirmed."}</p>
      <form onSubmit={e => { e.preventDefault(); void submit(); }}>
        <label>Local search question <textarea value={question} onChange={e => { resetSearch(); setQuestion(e.target.value); }} /></label>
        <p>{bytes} / 4096 UTF-8 bytes.</p>
        <label>Result limit <select value={limit} onChange={e => { resetSearch(); setLimit(Number(e.target.value)); }}>{[8, 16, 32].map(n => <option key={n} value={n}>{n}</option>)}</select></label>
        <button disabled={busy || !project.initialized || !question.trim() || bytes > 4096}>Search local cache</button>
      </form>
    </> : null}
    {busy ? <p role="status">Searching this subscription locally…</p> : null}
    {result ? <section aria-label="Cache search results">
      <h3>Read-only excerpts</h3>
      {!result.hits.length ? <p>No local excerpts match this bounded search.</p> : null}
      {result.hits.map((hit, i) => <article key={hit.stream_id}>
        <h4>[{i + 1}] {hit.summary}</h4>
        <p style={{ whiteSpace: "pre-wrap" }}>{hit.text}</p>
        <p>Author {hit.author}; authored {hit.authored_at}; canonical {hit.stream_id}; {hit.shared_version !== undefined ? `shared version ${hit.shared_version}` : "curated project replica"}; digest {hit.digest}.</p>
        <p>Source record {hit.source_record_id}; source version {hit.source_version}; {hit.locator ? `locator (text only): ${hit.locator}` : "private project replica; no publication locator"}.</p>
        {!!hit.unavailable_source_ids?.length && <p>{hit.unavailable_source_ids.length} provenance references retained; source content is not downloaded.</p>}
        <h5>Reviewed evidence</h5>
        {hit.evidence.length ? hit.evidence.map((e, n) => <blockquote key={`${e.digest}/${n}`}>
          <p>{e.label}: {e.text}</p><p>Digest {e.digest}; reviewer {e.reviewer}; reviewed {e.reviewed_at}.</p>
        </blockquote>) : <p>No attached evidence.</p>}
      </article>)}
    </section> : null}
  </section>;
}
