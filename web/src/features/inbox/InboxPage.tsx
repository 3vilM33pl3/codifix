import { useEffect, useRef, useState } from "react";
import { AuthIdentityError, authLoginUrl, getAuthMe, inboxRequest } from "../../api";
import type { Conversation, ConversationDetail } from "./types";
import "./inbox.css";

const inboxUrl = "/?inbox=1";
const conversationUrl = (id: string) => `${inboxUrl}&conversation=${encodeURIComponent(id)}`;
const date = (value: string) => new Date(value).toLocaleString();

export function InboxPage({ conversationId }: { conversationId: string | null }) {
  const [list, setList] = useState<Conversation[] | null>(null);
  const [detail, setDetail] = useState<ConversationDetail | null>(null);
  const [reload, setReload] = useState(0);
  const [busy, setBusy] = useState(false);
  const [sending, setSending] = useState(false);
  const [readOnly, setReadOnly] = useState(true);
  const [signIn, setSignIn] = useState(false);
  const [error, setError] = useState("");

  function report(failure: unknown) {
    if (failure instanceof AuthIdentityError) {
      setList(null);
      setDetail(null);
      setSignIn(failure.status === 401);
    }
    setError(failure instanceof Error ? failure.message : "Could not reach the Inbox.");
  }

  useEffect(() => {
    const controller = new AbortController();
    const { signal } = controller;
    async function load() {
      setBusy(true);
      setError("");
      try {
        const auth = await getAuthMe(true);
        if (signal.aborted) return;
        if (!auth.authenticated || auth.principal.kind !== "human_oidc") throw new AuthIdentityError(401);
        setSignIn(false);
        setReadOnly(auth.read_only);
        if (conversationId) {
          const current = await inboxRequest<ConversationDetail>(`/${encodeURIComponent(conversationId)}`, signal);
          if (signal.aborted) return;
          setDetail(current);
          const last = current.messages.at(-1);
          if (last && current.conversation.unread_count > 0 && !auth.read_only) {
            const read = await inboxRequest<ConversationDetail>(`/${encodeURIComponent(conversationId)}/read`, signal, { sequence: last.sequence });
            if (!signal.aborted) setDetail(read);
          }
        } else {
          const conversations = await inboxRequest<Conversation[]>("", signal);
          if (!signal.aborted) setList(conversations);
        }
      } catch (failure) {
        if (!signal.aborted) report(failure);
      } finally {
        if (!signal.aborted) setBusy(false);
      }
    }
    void load();
    return () => controller.abort();
  }, [conversationId, reload]);

  return <main className="phone-inbox">
    <header><a href={conversationId ? inboxUrl : "/"}>{conversationId ? "‹ Inbox" : "‹ Codifix"}</a>
      <button type="button" disabled={busy || sending} onClick={() => setReload(value => value + 1)}>Refresh</button></header>
    <h1>{detail?.conversation.title ?? "Inbox"}</h1>
    {busy && <p role="status">Loading…</p>}
    {error && <p role="alert">{error}</p>}
    {signIn && <a className="button-link" href={authLoginUrl(conversationId ? conversationUrl(conversationId) : inboxUrl)}>Sign in</a>}
    {list && (list.length ? <ul className="inbox-list">{list.map(chat => <li key={chat.id}>
      <a href={conversationUrl(chat.id)}><strong>{chat.title}</strong><small>{date(chat.updated_at)}{chat.paused && " · Paused"}{chat.unread_count > 0 && ` · ${chat.unread_count} unread`}</small></a>
    </li>)}</ul> : <p>No conversations yet.</p>)}
    {detail && <ConversationView detail={detail} update={value => { setDetail(value); setError(""); }} readOnly={busy || readOnly || sending} setSending={setSending} report={report} />}
  </main>;
}

function ConversationView({ detail, update, readOnly, setSending, report }: {
  detail: ConversationDetail;
  update: (detail: ConversationDetail) => void;
  readOnly: boolean;
  setSending: (sending: boolean) => void;
  report: (error: unknown) => void;
}) {
  const { conversation, members, messages, tasks } = detail;
  const unavailable = readOnly || conversation.paused;
  return <>
    {conversation.paused && <p role="status">Conversation paused</p>}
    <h2>Messages</h2>
    {messages.length === 0 && <p>No messages yet.</p>}
    <ol className="inbox-list">{messages.map(message => <li key={message.id}>
      <strong>{message.role === "human" ? "You" : members.find(member => member.id === message.member_id)?.specialist.display_name ?? message.role}</strong>
      <p>{message.body}</p>
      {message.role === "system" && typeof message.artifact?.protected_unseal_url === "string" && message.artifact.protected_unseal_url.startsWith("https://") &&
        <a href={message.artifact.protected_unseal_url} rel="noreferrer">Open protected unseal page</a>}
      <small>{date(message.created_at)} · {message.delivery}</small>
    </li>)}</ol>
    {tasks.length > 0 && <section><h2>Tasks</h2>{tasks.map(task => <article key={task.id}>
      <strong>{task.objective}</strong><small>{task.status.replaceAll("_", " ")}</small>
      {task.question && <><p>{task.question}</p><ReplyForm taskId={task.id} conversationId={conversation.id}
        disabled={unavailable} setSending={setSending} update={update} report={report} /></>}
    </article>)}</section>}
    <ReplyForm conversationId={conversation.id} disabled={unavailable || !conversation.lead_member_id}
      setSending={setSending} update={update} report={report} />
  </>;
}

function ReplyForm({ conversationId, taskId, disabled, setSending, update, report }: {
  conversationId: string; taskId?: string; disabled: boolean;
  setSending: (sending: boolean) => void;
  update: (detail: ConversationDetail) => void;
  report: (error: unknown) => void;
}) {
  const [body, setBody] = useState("");
  const lastAttempt = useRef<{ id: string; body: string } | null>(null);
  const inflight = useRef(false);
  const label = taskId ? "Answer clarification" : "Reply to conversation";
  async function submit() {
    if (disabled || inflight.current || !body.trim()) return;
    inflight.current = true;
    setSending(true);
    try {
      if (lastAttempt.current?.body !== body) lastAttempt.current = { id: crypto.randomUUID(), body };
      const payload = taskId ? { client_message_id: lastAttempt.current.id, answer: body }
        : { client_message_id: lastAttempt.current.id, body, format: "text" };
      const suffix = taskId ? `tasks/${encodeURIComponent(taskId)}/respond` : "messages";
      const updated = await inboxRequest<ConversationDetail>(`/${encodeURIComponent(conversationId)}/${suffix}`, undefined, payload);
      update(updated);
      setBody("");
      lastAttempt.current = null;
    } catch (failure) { report(failure); }
    finally { inflight.current = false; setSending(false); }
  }
  return <form onSubmit={event => { event.preventDefault(); void submit(); }}>
    <label>{label}<textarea maxLength={32768} disabled={disabled} value={body} onChange={event => setBody(event.target.value)} /></label>
    <button disabled={disabled || !body.trim()}>{taskId ? "Send answer" : "Send reply"}</button>
  </form>;
}
