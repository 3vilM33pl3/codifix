import { useCallback, useEffect, useState } from "react";
import { onboardingRequest } from "../../api";
import type { MemoryArea, OnboardingManifest, OnboardingManifestEntry } from "./types";

const blankEntry: OnboardingManifestEntry = {
  area: "workshop", owner_instance_id: "", repository_url: "", default_branch: "",
  offline_allowed: false, tools: [], destinations: [],
};

function reviewedEntry(entry: OnboardingManifestEntry, toolsJson: string, destinationsJson: string): OnboardingManifestEntry {
  const tools: unknown = JSON.parse(toolsJson);
  const destinations: unknown = JSON.parse(destinationsJson);
  if (!Array.isArray(tools) || !tools.every((tool) => tool && typeof tool === "object"
    && ["name", "version", "url", "sha256"].every((key) => typeof tool[key] === "string" && tool[key].trim())
    && /^[a-fA-F0-9]{64}$/.test(tool.sha256))) throw new Error("Every tool requires name, pinned version, URL and a 64-character SHA-256 digest.");
  if (!Array.isArray(destinations) || !destinations.every((destination) => destination && typeof destination === "object"
    && ["name", "provider_id", "destination_id"].every((key) => typeof destination[key] === "string" && destination[key].trim()))) throw new Error("Every destination requires a name, reviewed provider ID and destination ID.");
  if (entry.area === "trust" && entry.offline_allowed) throw new Error("Trust is remote-only; offline copies are not allowed.");
  return { ...entry, tools: tools as OnboardingManifestEntry["tools"], destinations: destinations as OnboardingManifestEntry["destinations"] };
}

export function ProjectsPanel({ readOnly }: { readOnly: boolean }) {
  const [projects, setProjects] = useState<OnboardingManifest[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [projectId, setProjectId] = useState("");
  const [revision, setRevision] = useState(0);
  const [entry, setEntry] = useState<OnboardingManifestEntry>({ ...blankEntry });
  const [toolsJson, setToolsJson] = useState("[]");
  const [destinationsJson, setDestinationsJson] = useState("[]");
  const [review, setReview] = useState<{ project_id: string; expected_revision: number; entry: OnboardingManifestEntry } | null>(null);
  const [confirmed, setConfirmed] = useState(false);

  const refresh = useCallback(async (signal?: AbortSignal) => {
    setLoading(true); setError(null);
    try {
      const result = await onboardingRequest<{ schema_version: 1; projects: OnboardingManifest[] }>("/projects", undefined, signal);
      if (!signal?.aborted) setProjects(result.projects);
    } catch (nextError) { if (!signal?.aborted) setError((nextError as Error).message); }
    finally { if (!signal?.aborted) setLoading(false); }
  }, []);
  useEffect(() => { const controller = new AbortController(); void refresh(controller.signal); return () => controller.abort(); }, [refresh]);

  function edit(manifest?: OnboardingManifest) {
    setProjectId(manifest?.project_id ?? ""); setRevision(manifest?.revision ?? 0);
    setEntry(manifest ? {
      area: manifest.area, owner_instance_id: manifest.owner_instance_id, repository_url: manifest.repository_url,
      default_branch: manifest.default_branch, offline_allowed: manifest.offline_allowed,
      tools: manifest.tools, destinations: manifest.destinations,
    } : { ...blankEntry });
    setToolsJson(JSON.stringify(manifest?.tools ?? [], null, 2));
    setDestinationsJson(JSON.stringify(manifest?.destinations ?? [], null, 2));
    setReview(null); setConfirmed(false); setStatus(null);
  }
  function prepareReview() {
    if (readOnly || loading || busy) return;
    setError(null); setConfirmed(false);
    try { setReview({ project_id: projectId.trim(), expected_revision: revision, entry: reviewedEntry(entry, toolsJson, destinationsJson) }); }
    catch (nextError) { setReview(null); setError((nextError as Error).message); }
  }
  async function save() {
    if (readOnly || busy || loading || !review || !confirmed) return;
    setBusy(true); setError(null); setStatus(null);
    try {
      const saved = await onboardingRequest<OnboardingManifest>(`/projects/${encodeURIComponent(review.project_id)}`, {
        expected_revision: review.expected_revision, entry: review.entry,
      });
      edit(saved); setStatus(`Manifest for ${saved.project} saved at revision ${saved.revision}.`);
      await refresh();
    } catch (nextError) { setError((nextError as Error).message); setConfirmed(false); }
    finally { setBusy(false); }
  }
  const disabled = readOnly || loading || busy;

  return <div className="panel-stack">
    <section className="panel access-heading"><div><h3>Projects</h3>
      <p>Review the local onboarding catalog and pinned tools. Catalog entries do not grant access or establish authority on the owning remote instance. Saving this manifest does not install software, enrol a Mac, or start selective sync.</p></div>
      <button type="button" disabled={loading || busy} onClick={() => { setReview(null); setConfirmed(false); void refresh(); }}>Refresh projects</button>
    </section>
    {error ? <div className="panel error-banner" role="alert">Projects error: {error}</div> : null}
    {status ? <div className="panel" role="status">{status}</div> : null}
    <section className="panel access-table-section"><h3>Reviewed manifests</h3><div className="table-scroll"><table>
      <thead><tr><th>Project / stable identity</th><th>Area</th><th>Owner instance</th><th>Repository / branch</th><th>Offline</th><th>Review</th><th /></tr></thead>
      <tbody>{projects.map((manifest) => <tr key={manifest.project_id}><td>{manifest.project}<br /><code>{manifest.project_id}</code></td>
        <td>{manifest.area}</td><td><code>{manifest.owner_instance_id}</code></td><td>{manifest.repository_url}<br />{manifest.default_branch}</td>
        <td>{manifest.area === "trust" ? "remote-only" : manifest.offline_allowed ? "allowed" : "not allowed"}</td>
        <td>Revision {manifest.revision}<br />{manifest.reviewed_by}<br />{manifest.reviewed_at}</td>
        <td><button type="button" disabled={busy} onClick={() => edit(manifest)}>Review {manifest.project}</button></td></tr>)}</tbody>
    </table></div>{!loading && !projects.length ? <p>No reviewed manifests.</p> : null}
      <button type="button" disabled={busy} onClick={() => edit()}>New manifest for existing project</button>
    </section>
    <section className="panel access-admin-form"><h3>{revision ? `Edit manifest revision ${revision}` : "Prepare onboarding manifest"}</h3>
      <p>The stable project UUID must already exist in this service. Area and owner instance are explicit policy choices.</p>
      <fieldset disabled={busy || Boolean(review)}>
        <label>Stable project UUID<input value={projectId} disabled={revision > 0} onChange={(event) => setProjectId(event.target.value)} /></label>
        <label>Memory area<select value={entry.area} onChange={(event) => { const area = event.target.value as MemoryArea; setEntry({ ...entry, area, offline_allowed: area === "trust" ? false : entry.offline_allowed }); }}>
          <option value="general">General</option><option value="workshop">Workshop</option><option value="homelab">Homelab</option><option value="trust">Trust — remote-only</option>
        </select></label>
        <label>Owner instance UUID<input value={entry.owner_instance_id} onChange={(event) => setEntry({ ...entry, owner_instance_id: event.target.value })} /></label>
        <label>Repository URL<input value={entry.repository_url} onChange={(event) => setEntry({ ...entry, repository_url: event.target.value })} /></label>
        <label>Default branch<input value={entry.default_branch} onChange={(event) => setEntry({ ...entry, default_branch: event.target.value })} /></label>
        <label className="access-checkbox"><input type="checkbox" checked={entry.offline_allowed} disabled={entry.area === "trust"} onChange={(event) => setEntry({ ...entry, offline_allowed: event.target.checked })} />Allow offline project data</label>
        {entry.area === "trust" ? <p role="status">Trust is remote-only. Offline copies are prohibited.</p> : null}
        <label>Pinned tools (JSON array)<textarea rows={5} value={toolsJson} onChange={(event) => setToolsJson(event.target.value)} /></label>
        <p>Each tool: name, version, url, sha256. Tools are pinned declarations; they are not executed here.</p>
        <label>Destinations (JSON array)<textarea rows={4} value={destinationsJson} onChange={(event) => setDestinationsJson(event.target.value)} /></label>
        <p>Each destination: name, provider_id, destination_id. Use reviewed identifiers for this project.</p>
      </fieldset>
      <button type="button" disabled={disabled || !projectId.trim() || !entry.owner_instance_id.trim() || !entry.repository_url.trim() || !entry.default_branch.trim()}
        onClick={prepareReview}>Review manifest changes</button>
    </section>
    {review ? <section className="panel access-manifest-review" aria-label="Manifest review"><h3>Review exact manifest</h3>
      <pre>{JSON.stringify(review, null, 2)}</pre>
      <label className="access-checkbox"><input type="checkbox" checked={confirmed} disabled={disabled} onChange={(event) => setConfirmed(event.target.checked)} />I reviewed this project identity, area, owner, repository, tools and destinations.</label>
      <div className="actions-row"><button type="button" disabled={disabled || !confirmed} onClick={() => void save()}>Save reviewed manifest</button>
        <button type="button" disabled={busy} onClick={() => { setReview(null); setConfirmed(false); }}>Continue editing</button></div>
    </section> : null}
  </div>;
}
