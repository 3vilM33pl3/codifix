import { invoke } from "@tauri-apps/api/core";
import { request } from "./service";

export type LinkedIdentity = {
  did: string;
  handle: string | null;
  linked_at: string;
};
export type NetworkStatus = {
  enabled: boolean;
  home_instance_id: string | null;
  identity: LinkedIdentity | null;
  identity_linking_available: boolean;
};
export type IdentityAttempt = {
  attempt_id: string;
  status: "pending" | "complete" | "expired" | "failed" | "cancelled";
  authorization_url: string | null;
  expires_at: string;
  error?: string | null;
};
export type Specialist = {
  id: string;
  instance_id: string;
  specialist_id: string;
  display_name: string;
  summary: string;
  capabilities: string[];
  enrolled: boolean;
  available: boolean;
  grant_id?: string | null;
  mandate_id?: string | null;
  remote: boolean;
};
export type Conversation = {
  id: string;
  title: string;
  lead_member_id: string | null;
  paused: boolean;
  revision: number;
  created_at: string;
  updated_at: string;
  unread_count: number;
};
export type Member = {
  id: string;
  specialist: Specialist;
  active: boolean;
  revision: number;
  joined_sequence: number;
  disclosure_summary: string;
  selected_message_ids: string[];
};
export type Message = {
  id: string;
  conversation_id: string;
  sequence: number;
  role: "human" | "specialist" | "system";
  member_id: string | null;
  body: string;
  format: "text" | "markdown" | "plan";
  artifact: unknown | null;
  created_at: string;
  delivery: "queued" | "confirmed" | "failed" | "uncertain";
  root_id: string;
  recipient_member_ids: string[];
};
export type TaskCard = {
  id: string;
  member_id: string;
  status: string;
  objective: string;
  question: string | null;
  result: unknown | null;
  artifacts: unknown[];
  cancel_requested: boolean;
  root_id: string;
};
export type ConversationDetail = {
  conversation: Conversation;
  members: Member[];
  messages: Message[];
  tasks: TaskCard[];
};
export type SendMessage = {
  client_message_id: string;
  body: string;
  format: Message["format"];
  artifact?: unknown;
  recipient_member_ids: string[];
};
export const inboxPath = (id: string, suffix = "") =>
  `/v1/inbox/conversations/${id}${suffix ? `/${suffix}` : ""}`;
export const networkStatus = () => request<NetworkStatus>("/v1/network/status");
export const specialists = () =>
  request<Specialist[]>("/v1/network/specialists");
export const conversations = () =>
  request<Conversation[]>("/v1/inbox/conversations");
export const conversation = (id: string) =>
  request<ConversationDetail>(inboxPath(id));
export type MessagePage = { messages: Message[]; has_more: boolean };
export const conversationHistory = (id: string, beforeSequence: number) =>
  request<MessagePage>(
    `${inboxPath(id, "messages")}?before_sequence=${beforeSequence}&limit=100`,
  );

// Match the service's UTF-8 context bound before fixing a retry ID. Character
// counts alone miss multibyte text and the serialized plan envelope.
export function messageValidation(
  body: string,
  artifact?: unknown,
): string | null {
  const bytes = (text: string) => new TextEncoder().encode(text).byteLength;
  const serialized = artifact == null ? null : JSON.stringify(artifact);
  if (bytes(body) > 32_768)
    return "The message exceeds 32 KiB. Shorten it before sending.";
  if (serialized && bytes(serialized) > 32_768)
    return "The structured plan exceeds 32 KiB. Split it into smaller plan revisions before sending.";
  if (serialized && bytes(`${body}\nPlan artifact:\n${serialized}`) > 32_768)
    return "The message and plan together exceed 32 KiB. Shorten the message or split the plan before sending.";
  return null;
}
export const openIdentityAttempt = (attemptId: string) =>
  invoke<void>("open_network_identity", { attemptId });
export const openNetworkLink = (url: string) =>
  invoke<void>("open_network_link", { url });

export function featureUnavailable(error: unknown) {
  return /HTTP (404|501)\b/.test(String(error));
}
export function canContact(specialist: Specialist) {
  return specialist.enrolled && specialist.available && !!specialist.grant_id;
}
export function safePublicLink(value: unknown): string | null {
  if (typeof value !== "string") return null;
  try {
    const url = new URL(value);
    return url.protocol === "https:" && !url.username && !url.password
      ? url.href
      : null;
  } catch {
    return null;
  }
}
