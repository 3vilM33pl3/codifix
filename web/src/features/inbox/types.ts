export interface Conversation {
  id: string;
  title: string;
  unread_count: number;
  updated_at: string;
  paused: boolean;
  lead_member_id: string | null;
}

export interface ConversationDetail {
  conversation: Conversation;
  members: { id: string; specialist: { display_name: string } }[];
  messages: {
    id: string;
    sequence: number;
    member_id: string | null;
    role: string;
    body: string;
    created_at: string;
    delivery: string;
    artifact?: { protected_unseal_url?: string } | null;
  }[];
  tasks: {
    id: string;
    objective: string;
    status: string;
    question: string | null;
  }[];
}
