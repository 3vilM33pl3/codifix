// Explicit development-only fixture. This module is never imported by main.tsx.
import { mockIPC } from "@tauri-apps/api/mocks";
import type { ConversationDetail, Message, Specialist } from "../network-api";
import { auth as localAuth } from "./fixtures";
if (!import.meta.env.DEV)
  throw new Error("The network review fixture is development-only.");
const id = (suffix: number) =>
  `00000000-0000-4000-8000-${String(suffix).padStart(12, "0")}`;
const now = () => new Date().toISOString();
const auth = {
  ...localAuth,
  mode: "multi_user",
  principal: {
    ...localAuth.principal,
    id: id(1),
    kind: "human_oidc",
    display_name: "Review fixture owner",
  },
};
const lead: Specialist = {
  id: id(2),
  instance_id: id(3),
  specialist_id: id(2),
  display_name: "Planning specialist",
  summary: "Turns agreed plans into trackable milestones and tickets.",
  capabilities: ["conversation_turn", "plan_prepare", "plan_apply"],
  enrolled: true,
  available: true,
  grant_id: id(4),
  remote: false,
};
const other: Specialist = {
  ...lead,
  id: id(5),
  specialist_id: id(5),
  display_name: "Documentation specialist",
  summary: "Writes and reviews guides with evidence from your project.",
  remote: true,
};
let detail: ConversationDetail = {
  conversation: {
    id: id(6),
    title: "Release roadmap",
    lead_member_id: id(7),
    paused: false,
    revision: 1,
    created_at: now(),
    updated_at: now(),
    unread_count: 0,
  },
  members: [
    {
      id: id(7),
      specialist: lead,
      active: true,
      revision: 1,
      joined_sequence: 0,
      disclosure_summary: "",
      selected_message_ids: [],
    },
  ],
  messages: [
    {
      id: id(8),
      conversation_id: id(6),
      sequence: 1,
      role: "human",
      member_id: null,
      body: "Plan the next release with documentation and acceptance milestones.",
      format: "markdown",
      artifact: null,
      created_at: now(),
      delivery: "confirmed",
      root_id: id(8),
      recipient_member_ids: [],
    },
  ],
  tasks: [],
};
localStorage.clear();
localStorage.setItem(
  "codifix.desktop.preferences",
  JSON.stringify({
    theme: "light",
    endpoint: "http://127.0.0.1:4250",
    connectionName: "Isolated Network review",
    notifications: false,
  }),
);
mockIPC((command, raw) => {
  if (command === "connect_service") return auth;
  if (command === "list_areas") return [];
  if (
    ["resolve_area", "select_custom_service", "disconnect_service"].includes(
      command,
    )
  )
    return null;
  if (command !== "api_request")
    throw new Error("Only isolated fixture actions are enabled.");
  const { path, body } = raw as {
    path: string;
    body?: Record<string, unknown>;
  };
  if (path === "/v1/auth/me") return auth;
  if (path === "/v1/network/status")
    return {
      enabled: true,
      home_instance_id: id(3),
      identity: null,
      identity_linking_available: false,
    };
  if (path === "/v1/network/specialists") return [lead, other];
  if (path === "/v1/inbox/conversations") return [detail.conversation];
  if (path === `/v1/inbox/conversations/${id(6)}` || path.endsWith("/read"))
    return detail;
  if (path.endsWith("/messages") && body) {
    if (
      !detail.messages.some((message) => message.id === body.client_message_id)
    ) {
      const message: Message = {
        id: String(body.client_message_id),
        conversation_id: id(6),
        sequence: detail.messages.length + 1,
        role: "human",
        member_id: null,
        body: String(body.body),
        format: body.format as Message["format"],
        artifact: body.artifact ?? null,
        created_at: now(),
        delivery: "confirmed",
        root_id: String(body.client_message_id),
        recipient_member_ids: body.recipient_member_ids as string[],
      };
      detail.messages.push(message, {
        ...message,
        id: crypto.randomUUID(),
        sequence: message.sequence + 1,
        role: "specialist",
        member_id: id(7),
        body: "I will draft the roadmap and check milestone acceptance criteria with the documentation specialist. This is an isolated fixture response.",
      });
      detail.tasks.push({
        id: crypto.randomUUID(),
        member_id: id(7),
        status: "working",
        objective: "Prepare release roadmap",
        question: null,
        result: null,
        artifacts: [],
        cancel_requested: false,
        root_id: message.root_id,
      });
    }
    return structuredClone(detail);
  }
  if (path.endsWith("/members") && body) {
    detail.members.push({
      id: id(9),
      specialist: other,
      active: true,
      revision: 1,
      joined_sequence: detail.messages.length,
      disclosure_summary: String(body.summary),
      selected_message_ids: body.selected_message_ids as string[],
    });
    return structuredClone(detail);
  }
  if (path.endsWith("/pause") && body) {
    detail.conversation.paused = !!body.paused;
    return structuredClone(detail);
  }
  if (path.endsWith("/cancel")) {
    detail.tasks.forEach((task) => {
      task.cancel_requested = true;
    });
    return structuredClone(detail);
  }
  throw new Error(`Unimplemented fixture route: ${path}`);
});
await import("../main");
