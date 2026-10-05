import { parseCompanyAgentEnvelope } from "./companyAgentTypes";

// Consume public metadata only. Never preserve it in login return-to or browser storage.
export function consumeAgentRequestLink(location: Location, history: History): { request?: string; notice?: string } {
  const query = new URLSearchParams(location.search);
  if (!query.has("agent_request")) return {};
  const values = query.getAll("agent_request");
  query.delete("agent_request");
  history.replaceState(null, "", `${location.pathname}${query.size ? `?${query}` : ""}${location.hash}`);
  try {
    if (values.length !== 1 || new TextEncoder().encode(values[0]).length > 8192) throw new Error();
    parseCompanyAgentEnvelope(values[0]);
    return { request: values[0], notice: "Public agent request removed from the URL. No automatic approval. If signed out, sign in and paste/import the native public envelope on Access Devices." };
  } catch { return { notice: "Invalid or oversized public agent link removed from the URL. Nothing approved; use the native public JSON paste/import workflow." }; }
}
