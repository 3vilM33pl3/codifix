export type Capability = "pull" | "publish" | "manage_membership" | "ask_expertise" | "answer_expertise";
export type RecordKey = { publisher: string; id: string };
export type Stream = { origin: string; canonical: string };
export type Content = { text: string; summary: string; memory_type: string; confidence: number; importance: number; tags: string[] };
export type Discovery = { space: string; instance: string; authority: string; principal: string; permissions: string[]; read_only: boolean; profile: string };
export type Page<T> = { items: T[]; next_offset: number | null };
export type Catalog = {
  as_of_sequence: number;
  streams: Page<{ stream: Stream; shared_version: number; state: { tombstone: boolean; status: string } | null }>;
  members: Page<{ member: string; capabilities: Capability[] }>;
  proposals: Page<{ key: RecordKey; target: Stream; author: string; resolution: string; base_version: number }>;
  local: { grants: Page<{ principal_id: string; capabilities: Capability[]; enabled: boolean }>; subscriptions: Page<{ canonical_id: string; principal_id: string; enabled: boolean; shared_version: number }>; reviews: Page<unknown> };
};
export type Status = { retained_through: number; observed_capabilities: Capability[]; local: { queued_intents: number; accepted_not_replicated: number; replicated_intents: number; blocked_intents: number; pending_source_changes: number; sync_error: string | null } };
export type Draft = { request: { request_id: string; canonical: string; expected: { content: Content }; evidence: unknown[]; expected_shared_version: number; confirm_future_updates: boolean }; current_shared_content: Content | null; warning: string };
export type Proposal = { key: RecordKey; proposal: { target: Stream; base_version: number; content: Content; evidence: RecordKey[] }; author: string; resolution: string; current_shared_version: number; locator: string; digest: unknown };
export type Citation = { stream: Stream; record: RecordKey; shared_version: number; source: { record_id: string; version: number }; author: string; authored_at: string; locator: string; digest: unknown; evidence: RecordKey[] };
type WireRecord =
  | { kind: "memory_version"; payload: { publication: { content: Content } } }
  | { kind: "resolve_proposal"; payload: { proposal: RecordKey } }
  | { kind: "proposal"; payload: { content: Content } }
  | { kind: "membership" | "revocation" | "canonical_state" | "evidence" | "relation"; payload: unknown };
export type SharedContext = { as_of_sequence: number; citations: Citation[]; records: { envelope: { publisher: string; record_id: string; record: WireRecord } }[]; untrusted: true; retained_copy: true; external_forwarding_requires_fresh_policy: true };
export function selectedContent(context: SharedContext): Content {
  const citation = context.citations[0];
  const record = (key: RecordKey) => context.records.find(({ envelope }) => envelope.publisher === key.publisher && envelope.record_id === key.id)?.envelope.record;
  let selected = citation && record(citation.record);
  if (selected?.kind === "resolve_proposal") selected = record(selected.payload.proposal);
  if (selected?.kind === "memory_version") return selected.payload.publication.content;
  if (selected?.kind === "proposal") return selected.payload.content;
  throw new Error("Missing cited shared content");
}
