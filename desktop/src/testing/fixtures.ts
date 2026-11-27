// Illustrative data for tests and the explicit dev-only review page. Never imported by main.tsx.
import type {
  AuthMeResponse,
  ProjectOverviewResponse,
  ProjectMemoryListItem,
  MemoryEntryResponse,
  ReplacementProposalRecord,
} from "../service";
export const auth: AuthMeResponse = {
  authenticated: true,
  mode: "single_user",
  read_only: false,
  principal: {
    id: "local",
    kind: "internal",
    display_name: "Local owner",
    groups: [],
    global_role: "admin",
    global_permissions: ["memory_read", "memory_curate", "loops_run"],
    projects: [{ project: "codifix", role: "admin", source: "local" }],
  },
};
export const memory: ProjectMemoryListItem = {
  id: "11111111-1111-4111-8111-111111111111",
  summary: "Four primary destinations",
  preview: "Organise Codifix around Home, Memories, Sessions and Activity.",
  memory_type: "decision",
  confidence: 0.95,
  importance: 0.8,
  status: "active",
  updated_at: "2026-10-25T10:42:00Z",
  tags: ["desktop", "navigation"],
  tag_count: 2,
  source_count: 1,
  canonical_id: "11111111-1111-4111-8111-111111111111",
  version_no: 1,
  is_tombstone: false,
};
export const detail: MemoryEntryResponse = {
  ...memory,
  project: "codifix",
  canonical_text:
    "Organise Codifix around Home, Memories, Sessions and Activity. Keep configuration and diagnostics available through settings and contextual details.",
  created_at: memory.updated_at,
  sources: [
    {
      id: "source-1",
      task_id: "design-review",
      file_path: null,
      git_commit: null,
      source_kind: "note",
      excerpt:
        "The desktop navigation proposal was reviewed in the source session.",
    },
  ],
  related_memories: [],
  embedding_spaces: [],
};
export const overview = {
  project: "codifix",
  service_status: "healthy",
  database_status: "healthy",
  active_memories: 3,
  sessions_total: 2,
  last_capture_at: "2026-10-25T10:42:00Z",
  pending_replacement_proposals: 1,
} as ProjectOverviewResponse;
export const proposal: ReplacementProposalRecord = {
  id: "22222222-2222-4222-8222-222222222222",
  project: "codifix",
  target_memory_id: memory.id,
  target_summary: "Four primary destinations",
  candidate_summary: "Keep the workspace focused",
  candidate_canonical_text:
    "Four destinations, one project context. Keep configuration in Settings.",
  candidate_memory_type: "decision",
  score: 0.92,
  policy: "balanced",
  reasons: ["Reviewed desktop navigation proposal"],
  created_at: "2026-10-25T10:48:00Z",
};
export const memories = [
  memory,
  {
    ...memory,
    id: "33333333-3333-4333-8333-333333333333",
    summary: "Project conventions",
    preview: "Keep changes small and reviewable.",
    memory_type: "convention" as const,
  },
  {
    ...memory,
    id: "44444444-4444-4444-8444-444444444444",
    summary: "Capture behaviour",
    preview: "Show when knowledge was last updated.",
    memory_type: "implementation" as const,
  },
];
export function fixture(path: string): unknown {
  if (path === "/v1/auth/me") return auth;
  if (path.endsWith("/overview")) return overview;
  if (path.includes("/memories"))
    return { project: "codifix", total: memories.length, items: memories };
  if (path.endsWith("/replacement-proposals"))
    return { project: "codifix", proposals: [proposal] };
  if (path.endsWith("/history"))
    return {
      project: "codifix",
      canonical_id: memory.canonical_id,
      versions: [detail],
    };
  if (path.startsWith("/v1/memory/"))
    return {
      ...detail,
      id: path.split("/").pop(),
      ...(memories.find((m) => path.endsWith(m.id)) ?? {}),
    };
  if (path === "/v1/agents")
    return {
      collected_at: "2026-10-25T10:48:00Z",
      sessions: [],
      orphan_ports: [],
      rate_limits: [],
    };
  if (path.includes("/activities"))
    return {
      project: "codifix",
      total_returned: 2,
      items: [
        {
          id: "a1",
          project: "codifix",
          kind: "memory_created",
          summary: "Decision captured",
          details: null,
          recorded_at: "2026-10-25T10:42:00Z",
        },
        {
          id: "a2",
          project: "codifix",
          kind: "replacement_proposed",
          summary: "Knowledge update needs review",
          details: null,
          recorded_at: "2026-10-25T10:48:00Z",
        },
      ],
    };
  if (path.endsWith("/resume"))
    return {
      project: "codifix",
      briefing:
        "Navigation proposal reviewed. Next: validate the Linux desktop layouts.",
      context_items: memories,
      warnings: [],
    };
  throw new Error(`No test response for ${path}`);
}
