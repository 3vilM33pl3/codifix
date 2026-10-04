import { useEffect, useRef, useState } from "react";
import { federationRequest } from "../../api";

type Exchange = {
  request_id: string;
  recipient: string;
  created_at: string;
  question_status: string | null;
  answer_status: string | null;
};

/** Project-scoped metadata only: question and answer bodies never enter this panel. */
export function AgentExchangesPanel({ project, space }: { project: string; space: string }) {
  const [items, setItems] = useState<Exchange[] | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const lifetime = useRef<AbortController | null>(null);
  useEffect(() => {
    const controller = new AbortController();
    lifetime.current = controller;
    return () => controller.abort();
  }, []);
  async function refresh() {
    if (busy) return;
    setBusy(true); setError(""); setItems(null);
    try {
      const page = await federationRequest<Exchange[]>(project, space, "/agent-questions", undefined, lifetime.current?.signal);
      if (!lifetime.current?.signal.aborted) setItems(page);
    } catch (cause) {
      if (!lifetime.current?.signal.aborted) setError(String(cause));
    } finally {
      if (!lifetime.current?.signal.aborted) setBusy(false);
    }
  }
  return <section aria-label="Watched agent exchanges">
    <h3>Watched agent exchanges</h3>
    <p>Latest twenty requests in this project. Only IDs, destinations and observed states appear here; no question or answer text. Unknown status is not healthy.</p>
    <button disabled={busy} onClick={() => void refresh()}>Refresh agent exchanges</button>
    {error ? <p role="alert">Agent exchange status unavailable: {error}</p> : null}
    {items && !items.length ? <p>No watched-agent questions recorded for this project.</p> : null}
    {items?.length ? <ul>{items.map(item => <li key={item.request_id}>
      Request <code>{item.request_id}</code> to {item.recipient}; {new Date(item.created_at).toLocaleString()}; question {item.question_status ?? "unknown"}; answer {item.answer_status ?? "unknown"}.
    </li>)}</ul> : null}
  </section>;
}
