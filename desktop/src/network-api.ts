import { invoke } from "@tauri-apps/api/core";
import { request } from "./service";

export type LinkedIdentity = {
  did: string;
  handle: string | null;
  linked_at: string;
};
export type NetworkStatus = {
  enabled: boolean;
  home_instance_id: string;
  identity: LinkedIdentity | null;
  identity_linking_available: boolean;
};
export type IdentityAttempt = {
  attempt_id: string;
  status: "pending" | "complete" | "expired" | "failed";
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
