import { useEffect, useRef, useState, type FormEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Badge, Button, Empty, Intro, Modal, Technical } from "./ui";
import { errorText, request, time, type AuthMeResponse } from "./service";
import {
  NetworkGate,
  networkKey,
  useMounted,
  type NetworkProps,
} from "./Network";
import {
  canContact,
  conversation,
  conversationHistory,
  conversations,
  inboxPath,
  networkStatus,
  messageValidation,
  openNetworkLink,
  safePublicLink,
  specialists,
  type ConversationDetail,
  type Member,
  type Message,
  type SendMessage,
  type Specialist,
  type TaskCard,
} from "./network-api";

export function useInboxUpdates({
  scope,
  auth,
  connected,
  notify,
}: Omit<NetworkProps, "onConnect"> & { notify: (text: string) => void }) {
  const key = networkKey(scope, auth?.principal.id ?? "signed-out");
  const human =
    connected && !!auth?.authenticated && auth.principal.kind === "human_oidc";
  const status = useQuery({
    queryKey: [...key, "status"],
    queryFn: networkStatus,
    enabled: human,
    retry: false,
    refetchInterval: human ? 30_000 : false,
  });
  const list = useQuery({
    queryKey: [...key, "conversations"],
    queryFn: conversations,
    enabled: human && status.data?.enabled === true,
    retry: false,
    refetchInterval: human && status.data?.enabled ? 10_000 : false,
  });
  const previous = useRef<{
    scope: string;
    unread: Map<string, number>;
  } | null>(null);
  const identity = `${scope}:${auth?.principal.id ?? "signed-out"}`;
  useEffect(() => {
    if (!list.data || !human) return;
    const prior = previous.current;
    previous.current = {
      scope: identity,
      unread: new Map(list.data.map((item) => [item.id, item.unread_count])),
    };
    if (prior?.scope !== identity) return;
    const changed = list.data.find(
      (item) => item.unread_count > (prior.unread.get(item.id) ?? 0),
    );
    if (changed) notify(`New specialist activity in ${changed.title}`);
  }, [list.data, identity, human]);
  return human && status.data?.enabled
    ? (list.data?.reduce((sum, item) => sum + item.unread_count, 0) ?? 0)
    : 0;
}

export function Inbox(props: NetworkProps & { onNetwork: () => void }) {
  return (
    <>
      <Intro
        title="Inbox"
        subtitle="Work privately with your trusted specialists."
      />
      <NetworkGate {...props}>
        {(_, auth) => (
          <InboxContents
            key={`${props.scope}:${auth.principal.id}`}
            {...props}
            auth={auth}
          />
        )}
      </NetworkGate>
    </>
  );
}

function InboxContents({
  scope,
  auth,
  connected,
  onNetwork,
}: NetworkProps & { auth: AuthMeResponse; onNetwork: () => void }) {
  const key = networkKey(scope, auth.principal.id);
  const client = useQueryClient();
  const mounted = useMounted();
  const list = useQuery({
    queryKey: [...key, "conversations"],
    queryFn: conversations,
    enabled: connected,
    retry: false,
    refetchInterval: connected ? 5000 : false,
  });
  const roster = useQuery({
    queryKey: [...key, "specialists"],
    queryFn: specialists,
    enabled: connected,
    retry: false,
    refetchInterval: connected ? 15_000 : false,
  });
  const [selected, setSelected] = useState("");
  const [creating, setCreating] = useState(false);
  const [title, setTitle] = useState("");
  const [specialistId, setSpecialistId] = useState("");
  const [createRequest, setCreateRequest] = useState<{
    client_request_id: string;
    title: string;
    specialist_id: string;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const candidates = roster.data?.filter(canContact) ?? [];
  const id = selected || list.data?.[0]?.id || "";
  const locked = !connected || auth.read_only;
  async function create(event: FormEvent) {
    event.preventDefault();
    if (locked || busy) return;
    const payload = createRequest ?? {
      client_request_id: crypto.randomUUID(),
      title: title.trim(),
      specialist_id: specialistId || candidates[0]?.id,
    };
    if (!payload.title || !payload.specialist_id) return;
    if (new TextEncoder().encode(payload.title).byteLength > 200) {
      setError(
        "The title is too long. Shorten it before creating the conversation.",
      );
      return;
    }
    setCreateRequest(payload);
    setBusy(true);
    setError("");
    try {
      const detail = await request<ConversationDetail>(
        "/v1/inbox/conversations",
        payload,
      );
      if (!mounted.current) return;
      client.setQueryData(
        [...key, "conversation", detail.conversation.id],
        detail,
      );
      setSelected(detail.conversation.id);
      setCreating(false);
      setTitle("");
      setCreateRequest(null);
      await client.invalidateQueries({ queryKey: [...key, "conversations"] });
    } catch (cause) {
      if (mounted.current)
        setError(
          `Conversation creation is unconfirmed. Retry this same request. ${errorText(cause)}`,
        );
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  return (
    <>
      <div className="inbox-layout">
        <aside className="inbox-list" aria-label="Conversations">
          <Button
            className="primary"
            disabled={locked || !candidates.length || !!roster.error}
            onClick={() => {
              setError("");
              setCreating(true);
            }}
          >
            New conversation
          </Button>
          {!candidates.length && !roster.isPending && (
            <p className="caption">
              No available specialist has an invocation grant.{" "}
              <Button className="ghost" onClick={onNetwork}>
                View Network
              </Button>
            </p>
          )}
          {list.isPending ? (
            <p role="status">Loading conversations…</p>
          ) : list.error ? (
            <div className="notice danger" role="alert">
              {errorText(list.error)}
              <Button onClick={() => void list.refetch()}>Refresh</Button>
            </div>
          ) : (
            list.data?.map((item) => (
              <Button
                className={`inbox-list-item ${item.id === id ? "selected" : ""}`}
                aria-pressed={item.id === id}
                key={item.id}
                onClick={() => setSelected(item.id)}
              >
                <span>
                  <strong>{item.title}</strong>
                  <span className="caption">
                    {item.paused ? "Paused" : time(item.updated_at)}
                  </span>
                </span>
                {item.unread_count > 0 && (
                  <Badge tone="positive">{item.unread_count} unread</Badge>
                )}
              </Button>
            ))
          )}
          {(list.data?.length ?? 0) >= 200 && (
            <p role="status" className="caption">
              Showing the 200 most recently updated conversations. Older
              conversations are retained by the service.
            </p>
          )}
        </aside>
        <div className="inbox-detail">
          {id ? (
            <ConversationView
              key={id}
              id={id}
              queryKey={key}
              connected={connected}
              readOnly={auth.read_only}
              specialists={roster.data ?? []}
            />
          ) : (
            !list.isPending && (
              <Empty title="Start a conversation">
                <p>
                  Choose an enrolled specialist. You can invite more specialists
                  after reviewing the context to share.
                </p>
              </Empty>
            )
          )}
        </div>
      </div>
      <Modal
        open={creating}
        onClose={() => !busy && setCreating(false)}
        title="New conversation"
        description="Your first specialist will lead the conversation. You can change the lead later."
      >
        <form className="form" onSubmit={(event) => void create(event)}>
          <label>
            Conversation title
            <input
              maxLength={200}
              required
              value={title}
              disabled={busy || !!createRequest}
              onChange={(event) => setTitle(event.target.value)}
            />
          </label>
          <label>
            Lead specialist
            <select
              value={specialistId || candidates[0]?.id || ""}
              disabled={busy || !!createRequest}
              onChange={(event) => setSpecialistId(event.target.value)}
            >
              {candidates.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.display_name}
                </option>
              ))}
            </select>
          </label>
          {error && (
            <p role="alert" className="notice warning">
              {error}
            </p>
          )}
          <Button
            type="submit"
            className="primary"
            disabled={locked || busy || !title.trim() || !candidates.length}
          >
            {busy
              ? "Creating conversation…"
              : createRequest
                ? "Retry same request"
                : "Create conversation"}
          </Button>
        </form>
      </Modal>
    </>
  );
}

function ConversationView({
  id,
  queryKey,
  connected,
  readOnly,
  specialists: roster,
}: {
  id: string;
  queryKey: string[];
  connected: boolean;
  readOnly: boolean;
  specialists: Specialist[];
}) {
  const client = useQueryClient();
  const mounted = useMounted();
  const detailKey = [...queryKey, "conversation", id];
  const detail = useQuery({
    queryKey: detailKey,
    queryFn: async () => {
      const result = await conversation(id);
      if (result.conversation.id !== id)
        throw new Error("The conversation response did not match the request.");
      return result;
    },
    enabled: connected,
    retry: false,
    refetchInterval: connected ? 3000 : false,
  });
  const [body, setBody] = useState("");
  const [format, setFormat] = useState<Message["format"]>("text");
  const [plan, setPlan] = useState("");
  const [recipients, setRecipients] = useState<string[]>([]);
  const [pendingSend, setPendingSend] = useState<SendMessage | null>(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [receipt, setReceipt] = useState("");
  const [joining, setJoining] = useState(false);
  const [remove, setRemove] = useState<Member | null>(null);
  const [timeline, setTimeline] = useState<Message[]>([]);
  const [historyBusy, setHistoryBusy] = useState(false);
  const [historyMore, setHistoryMore] = useState(true);
  const [historyError, setHistoryError] = useState("");
  const readSequence = useRef(0);
  const data = detail.data;
  const active = data?.members.filter((member) => member.active) ?? [];
  const lead = active.find(
    (member) => member.id === data?.conversation.lead_member_id,
  );
  const availableLead = lead && canContact(lead.specialist);
  const locked = !connected || readOnly || !!detail.error || !data;
  const sendLocked =
    locked ||
    !!busy ||
    data?.conversation.paused ||
    (!recipients.length && !availableLead);
  const candidates = roster.filter(
    (specialist) =>
      canContact(specialist) &&
      !active.some((member) => member.specialist.id === specialist.id),
  );
  useEffect(() => {
    if (data) setTimeline((current) => mergeMessages(current, data.messages));
  }, [data?.messages]);
  async function loadOlder() {
    const before = timeline[0]?.sequence;
    if (!connected || historyBusy || !before || before <= 1) return;
    setHistoryBusy(true);
    setHistoryError("");
    try {
      const page = await conversationHistory(id, before);
      if (!mounted.current) return;
      if (
        page.messages.some(
          (message) =>
            message.conversation_id !== id || message.sequence >= before,
        )
      )
        throw new Error(
          "The history response did not match this conversation and cursor.",
        );
      setTimeline((current) => mergeMessages(current, page.messages));
      setHistoryMore(page.has_more);
    } catch (cause) {
      if (mounted.current) setHistoryError(errorText(cause));
    } finally {
      if (mounted.current) setHistoryBusy(false);
    }
  }
  useEffect(() => {
    if (
      !data ||
      !connected ||
      readOnly ||
      document.visibilityState === "hidden"
    )
      return;
    const sequence = data.messages.at(-1)?.sequence ?? 0;
    if (sequence <= readSequence.current) return;
    readSequence.current = sequence;
    void request<ConversationDetail>(inboxPath(id, "read"), { sequence })
      .then(() => {
        if (mounted.current)
          void client.invalidateQueries({
            queryKey: [...queryKey, "conversations"],
          });
      })
      .catch(() => {
        if (mounted.current) readSequence.current = 0;
      });
  }, [data?.messages, connected, readOnly, id]);
  useEffect(() => {
    if (
      !pendingSend ||
      !data?.messages.some(
        (message) => message.id === pendingSend.client_message_id,
      )
    )
      return;
    setPendingSend(null);
    setBody("");
    setPlan("");
    setRecipients([]);
    setError("");
    setReceipt("Message saved. Specialist progress will appear below.");
  }, [data?.messages, pendingSend]);

  async function update(suffix: string, payload: unknown, success: string) {
    setBusy(suffix);
    setError("");
    setReceipt("");
    try {
      const result = await request<ConversationDetail>(
        inboxPath(id, suffix),
        payload,
      );
      if (!mounted.current) return false;
      if (result.conversation.id !== id)
        throw new Error("The conversation receipt did not match.");
      client.setQueryData(detailKey, result);
      setReceipt(success);
      await client.invalidateQueries({
        queryKey: [...queryKey, "conversations"],
      });
      return true;
    } catch (cause) {
      if (mounted.current)
        setError(
          `The operation is unconfirmed. Refresh to reconcile before retrying. ${errorText(cause)}`,
        );
      return false;
    } finally {
      if (mounted.current) setBusy("");
    }
  }
  async function send(event: FormEvent) {
    event.preventDefault();
    if (sendLocked) return;
    let artifact: unknown;
    try {
      if (format === "plan") artifact = JSON.parse(plan);
    } catch {
      setError("The plan must be valid JSON. Nothing was sent.");
      return;
    }
    const payload = pendingSend ?? {
      client_message_id: crypto.randomUUID(),
      body: body.trim(),
      format,
      ...(format === "plan" ? { artifact } : {}),
      recipient_member_ids: recipients,
    };
    if (!payload.body) return;
    const validation = messageValidation(payload.body, payload.artifact);
    if (validation) {
      setError(`${validation} Nothing was sent.`);
      return;
    }
    setPendingSend(payload);
    if (
      await update(
        "messages",
        payload,
        "Message saved. Specialist progress will appear below.",
      )
    ) {
      setPendingSend(null);
      setBody("");
      setPlan("");
      setRecipients([]);
    }
  }
  if (!data)
    return detail.error ? (
      <div role="alert" className="notice danger">
        {errorText(detail.error)}
        <Button onClick={() => void detail.refetch()}>Refresh</Button>
      </div>
    ) : (
      <p role="status">Loading conversation…</p>
    );
  return (
    <div className="conversation-stack">
      <header className="row">
        <div>
          <h2>{data.conversation.title}</h2>
          <p className="caption">
            Private conversation · Everyone listed can receive shared messages
          </p>
        </div>
        <div className="actions">
          <Button
            disabled={locked || !!busy}
            onClick={() =>
              void update(
                "pause",
                { paused: !data.conversation.paused },
                data.conversation.paused
                  ? "Conversation resumed."
                  : "Conversation paused. Already admitted work may finish.",
              )
            }
          >
            {data.conversation.paused ? "Resume" : "Pause"}
          </Button>
          <Button
            disabled={locked || !!busy || !candidates.length}
            onClick={() => setJoining(true)}
          >
            Add specialist
          </Button>
        </div>
      </header>
      <section className="conversation-members" aria-label="Participants">
        {active.map((member) => (
          <div className="member" key={member.id}>
            <div>
              <strong>{member.specialist.display_name}</strong>
              <span className="caption">
                {member.id === data.conversation.lead_member_id
                  ? "Lead · "
                  : ""}
                {canContact(member.specialist) ? "Available" : "Unavailable"}
              </span>
            </div>
            <div className="actions">
              {member.id !== data.conversation.lead_member_id && (
                <Button
                  className="ghost"
                  disabled={locked || !!busy || !canContact(member.specialist)}
                  onClick={() =>
                    void update(
                      "lead",
                      { member_id: member.id },
                      `${member.specialist.display_name} is now the lead.`,
                    )
                  }
                >
                  Make lead
                </Button>
              )}
              <Button
                className="ghost"
                disabled={locked || !!busy}
                onClick={() => setRemove(member)}
              >
                Remove
              </Button>
            </div>
          </div>
        ))}
      </section>
      {!availableLead && (
        <div className="notice warning" role="status">
          The lead is unavailable. Choose an available lead or mention an
          available participant to continue.
        </div>
      )}
      {data.conversation.paused && (
        <div className="notice warning" role="status">
          Paused. New specialist work will not start until you resume. Already
          admitted effects may finish.
        </div>
      )}
      {detail.error && (
        <div role="alert" className="notice warning">
          Showing the last confirmed conversation. {errorText(detail.error)}{" "}
          <Button onClick={() => void detail.refetch()}>Refresh</Button>
        </div>
      )}
      {receipt && (
        <div role="status" className="notice positive">
          {receipt}
        </div>
      )}
      {error && (
        <div role="alert" className="notice warning">
          {error}
          <Button onClick={() => void detail.refetch()}>
            Refresh conversation
          </Button>
        </div>
      )}
      <section
        className="transcript"
        aria-label="Conversation messages"
        aria-live="polite"
      >
        {historyMore && (timeline[0]?.sequence ?? 0) > 1 && (
          <div className="row">
            <p className="caption">Earlier messages are available.</p>
            <Button
              disabled={!connected || historyBusy}
              onClick={() => void loadOlder()}
            >
              {historyBusy
                ? "Loading earlier messages…"
                : "Load earlier messages"}
            </Button>
          </div>
        )}
        {historyError && (
          <p role="alert" className="notice warning">
            {historyError}
          </p>
        )}
        {!timeline.length && (
          <Empty title="What would you like to work on?">
            <p>
              Ask a question, share a plan or describe an outcome. Specialists
              act within their standing grants.
            </p>
          </Empty>
        )}
        {timeline.map((message) => (
          <MessageView
            key={message.id}
            message={message}
            members={data.members}
          />
        ))}
        {pendingSend &&
          !timeline.some(
            (message) => message.id === pendingSend.client_message_id,
          ) && (
            <article className="message human pending" role="status">
              <div className="row">
                <strong>You</strong>
                <Badge tone="warning">
                  {busy === "messages" ? "Saving…" : "Delivery unconfirmed"}
                </Badge>
              </div>
              <p className="prose">{pendingSend.body}</p>
              <p className="caption">
                Retry uses the same message ID. Refresh also checks whether the
                message was saved.
              </p>
            </article>
          )}
      </section>
      {!!data.tasks.length && (
        <section className="conversation-stack" aria-label="Linked tasks">
          <h3>Specialist work</h3>
          {data.tasks.map((task) => (
            <TaskView
              key={task.id}
              task={task}
              member={data.members.find(
                (member) => member.id === task.member_id,
              )}
              locked={locked || !!busy}
              onUpdate={update}
            />
          ))}
        </section>
      )}
      <form className="composer" onSubmit={(event) => void send(event)}>
        <fieldset
          disabled={locked || !!busy || !!pendingSend}
          className="recipient-picker"
        >
          <legend>
            Ask a specialist{" "}
            <span className="caption">
              Mentions choose responders, not who can see the message.
            </span>
          </legend>
          <label>
            <input
              type="checkbox"
              checked={!recipients.length}
              onChange={() => setRecipients([])}
            />
            Lead{lead ? `: ${lead.specialist.display_name}` : " unavailable"}
          </label>
          {active.map((member) => (
            <label key={member.id}>
              <input
                type="checkbox"
                disabled={!canContact(member.specialist)}
                checked={recipients.includes(member.id)}
                onChange={(event) =>
                  setRecipients((current) =>
                    event.target.checked
                      ? [...current, member.id]
                      : current.filter((item) => item !== member.id),
                  )
                }
              />
              @{member.specialist.display_name}
            </label>
          ))}
        </fieldset>
        <label>
          Message
          <textarea
            rows={4}
            maxLength={32000}
            value={body}
            onChange={(event) => setBody(event.target.value)}
            disabled={locked || !!busy || !!pendingSend}
            placeholder="Ask a question or describe the work…"
            required
          />
        </label>
        <div className="composer-format">
          <label>
            Format
            <select
              value={format}
              onChange={(event) =>
                setFormat(event.target.value as Message["format"])
              }
              disabled={locked || !!busy || !!pendingSend}
            >
              <option value="text">Plain text</option>
              <option value="markdown">Markdown plan</option>
              <option value="plan">Structured plan JSON</option>
            </select>
          </label>
          <Button
            className="primary"
            type="submit"
            disabled={
              sendLocked || !body.trim() || (format === "plan" && !plan.trim())
            }
          >
            {busy === "messages"
              ? "Saving message…"
              : pendingSend
                ? "Retry same message"
                : "Send message"}
          </Button>
        </div>
        {format === "plan" && (
          <label>
            Structured plan
            <textarea
              rows={8}
              maxLength={32768}
              value={plan}
              onChange={(event) => setPlan(event.target.value)}
              disabled={locked || !!busy || !!pendingSend}
              placeholder='{"schema_version": 1, …}'
              required
            />
            <span className="caption">
              Paste the existing Codifix plan document. The specialist validates
              it before execution.
            </span>
          </label>
        )}
        <p className="caption">
          Private task data stays on authenticated channels. Never paste
          credentials or OpenBao unseal shares here.
        </p>
      </form>
      <JoinDialog
        open={joining}
        onClose={() => setJoining(false)}
        candidates={candidates}
        detail={{ ...data, messages: timeline }}
        locked={locked || !!busy}
        onUpdate={update}
      />
      <Modal
        open={!!remove}
        onClose={() => setRemove(null)}
        title="Remove specialist"
        description="Removal stops future admission and disclosure. Previously delivered context cannot be recalled; already admitted work may finish."
      >
        <p>Remove {remove?.specialist.display_name}?</p>
        <Button
          disabled={locked || !!busy}
          onClick={async () => {
            if (
              remove &&
              (await update(
                `members/${remove.id}/remove`,
                {},
                "Specialist removed. Remote cancellation may still be pending.",
              ))
            )
              setRemove(null);
          }}
        >
          Remove specialist
        </Button>
      </Modal>
    </div>
  );
}

function mergeMessages(previous: Message[], incoming: Message[]): Message[] {
  const byId = new Map(previous.map((message) => [message.id, message]));
  incoming.forEach((message) => byId.set(message.id, message));
  return Array.from(byId.values()).sort((a, b) => a.sequence - b.sequence);
}

function MessageView({
  message,
  members,
}: {
  message: Message;
  members: Member[];
}) {
  const author =
    message.role === "human"
      ? "You"
      : message.role === "system"
        ? "Codifix"
        : (members.find((member) => member.id === message.member_id)?.specialist
            .display_name ?? "Specialist");
  return (
    <article className={`message ${message.role}`}>
      <div className="row">
        <strong>{author}</strong>
        <span className="actions">
          <span className="caption">{time(message.created_at)}</span>
          <Badge
            tone={
              message.delivery === "failed" || message.delivery === "uncertain"
                ? "warning"
                : ""
            }
          >
            {message.delivery === "confirmed" ? "Saved" : message.delivery}
          </Badge>
        </span>
      </div>
      {message.format === "markdown" ? (
        <pre className="message-markdown">{message.body}</pre>
      ) : (
        <p className="prose">{message.body}</p>
      )}
      {message.artifact != null && (
        <Technical label="Shared plan artifact" value={message.artifact} />
      )}
    </article>
  );
}

function JoinDialog({
  open,
  onClose,
  candidates,
  detail,
  locked,
  onUpdate,
}: {
  open: boolean;
  onClose: () => void;
  candidates: Specialist[];
  detail: ConversationDetail;
  locked: boolean;
  onUpdate: (
    suffix: string,
    body: unknown,
    success: string,
  ) => Promise<boolean>;
}) {
  const [specialist, setSpecialist] = useState("");
  const [summary, setSummary] = useState("");
  const [selected, setSelected] = useState<string[]>([]);
  const [confirmed, setConfirmed] = useState(false);
  const [attempt, setAttempt] = useState<{
    client_request_id: string;
    specialist_id: string;
    summary: string;
    selected_message_ids: string[];
    confirmed: true;
  } | null>(null);
  const [inviteError, setInviteError] = useState("");
  const changed = () => setConfirmed(false);
  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Share context with a specialist"
      description="Review exactly what this specialist will receive. Earlier messages are excluded unless selected; subsequent shared messages are available after joining."
    >
      <form
        className="form"
        onSubmit={async (event) => {
          event.preventDefault();
          if (locked || !confirmed) return;
          const payload = attempt ?? {
            client_request_id: crypto.randomUUID(),
            specialist_id: specialist || candidates[0]?.id,
            summary: summary.trim(),
            selected_message_ids: selected,
            confirmed: true as const,
          };
          const selectedMessages = detail.messages.filter((message) =>
            payload.selected_message_ids.includes(message.id),
          );
          const bytes = (text: string) =>
            new TextEncoder().encode(text).byteLength;
          if (
            bytes(payload.summary) > 16_384 ||
            selectedMessages.length > 32 ||
            bytes(
              JSON.stringify({
                summary: payload.summary,
                selected: selectedMessages,
                watermark: detail.messages.at(-1)?.sequence ?? 0,
                recipient: payload.specialist_id,
              }),
            ) > 64_512
          ) {
            setInviteError(
              "This context is too large. Shorten the summary or select fewer messages before sharing.",
            );
            return;
          }
          setAttempt(payload);
          setInviteError("");
          if (
            await onUpdate(
              "members",
              payload,
              "Specialist added with the confirmed context snapshot.",
            )
          ) {
            setAttempt(null);
            setSpecialist("");
            setSummary("");
            setSelected([]);
            setConfirmed(false);
            onClose();
          } else {
            setInviteError(
              "Invitation delivery is unconfirmed. Retry this same invitation, or close this dialog and refresh the conversation to reconcile it.",
            );
          }
        }}
      >
        <label>
          Specialist
          <select
            value={specialist || candidates[0]?.id || ""}
            disabled={locked || !!attempt}
            onChange={(event) => {
              setSpecialist(event.target.value);
              changed();
            }}
          >
            {candidates.map((item) => (
              <option key={item.id} value={item.id}>
                {item.display_name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Summary to share
          <textarea
            value={summary}
            rows={5}
            maxLength={16000}
            placeholder="Describe the goal and context this specialist needs…"
            disabled={locked || !!attempt}
            onChange={(event) => {
              setSummary(event.target.value);
              changed();
            }}
            required
          />
        </label>
        {!!detail.messages.length && (
          <fieldset
            disabled={locked || !!attempt}
            className="disclosure-messages"
          >
            <legend>Include selected earlier messages</legend>
            {detail.messages.map((message) => (
              <label key={message.id}>
                <input
                  type="checkbox"
                  checked={selected.includes(message.id)}
                  disabled={
                    !selected.includes(message.id) && selected.length >= 32
                  }
                  onChange={(event) => {
                    setSelected((current) =>
                      event.target.checked
                        ? [...current, message.id]
                        : current.filter((id) => id !== message.id),
                    );
                    changed();
                  }}
                />
                <span>
                  {message.body}
                  {message.artifact != null && (
                    <Technical
                      label="Included artifact"
                      value={message.artifact}
                    />
                  )}
                </span>
              </label>
            ))}
          </fieldset>
        )}
        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={confirmed}
            disabled={locked || !!attempt}
            onChange={(event) => setConfirmed(event.target.checked)}
          />
          I reviewed this summary and {selected.length} selected messages for
          sharing with this specialist.
        </label>
        <p className="caption">
          This snapshot grants context only. The specialist still needs an
          active invocation grant for its work.
        </p>
        {inviteError && (
          <p role="alert" className="notice warning">
            {inviteError}
          </p>
        )}
        <Button
          type="submit"
          className="primary"
          disabled={
            locked || !summary.trim() || !confirmed || !candidates.length
          }
        >
          {attempt
            ? "Retry same invitation"
            : "Confirm context and add specialist"}
        </Button>
      </form>
    </Modal>
  );
}

function artifactLinks(value: unknown, depth = 0): string[] {
  if (depth > 5 || value == null) return [];
  if (Array.isArray(value))
    return value
      .slice(0, 100)
      .flatMap((item) => artifactLinks(item, depth + 1));
  if (typeof value !== "object") return [];
  return Object.entries(value)
    .slice(0, 100)
    .flatMap(([name, item]) => {
      const link = ["url", "web_url", "external_url"].includes(name)
        ? safePublicLink(item)
        : null;
      return link ? [link] : artifactLinks(item, depth + 1);
    });
}

function TaskView({
  task,
  member,
  locked,
  onUpdate,
}: {
  task: TaskCard;
  member?: Member;
  locked: boolean;
  onUpdate: (
    suffix: string,
    body: unknown,
    success: string,
  ) => Promise<boolean>;
}) {
  const [answer, setAnswer] = useState("");
  const [reply, setReply] = useState<{
    client_message_id: string;
    answer: string;
  } | null>(null);
  const [linkError, setLinkError] = useState("");
  const [replyError, setReplyError] = useState("");
  const terminal = [
    "completed",
    "succeeded",
    "failed",
    "cancelled",
    "canceled",
    "rejected",
  ].includes(task.status);
  const recovery = member?.specialist.capabilities.some(
    (capability) =>
      capability.includes("openbao") || capability.includes("unseal"),
  );
  const links = Array.from(
    new Set(artifactLinks([task.result, ...task.artifacts])),
  );
  return (
    <article className="card task-card">
      <div className="row">
        <h3>{task.objective}</h3>
        <Badge tone={task.cancel_requested ? "warning" : ""}>
          {task.cancel_requested && !terminal
            ? "Cancellation pending"
            : task.status.replaceAll("_", " ")}
        </Badge>
      </div>
      <p className="caption">
        {member?.specialist.display_name ?? "Specialist"}
      </p>
      {recovery && (
        <p className="notice">
          Recovery runs through the protected OpenBao controller and custodian
          channels. Unseal shares are never requested in this inbox.
        </p>
      )}
      {task.question && !terminal && !recovery && (
        <form
          className="form"
          onSubmit={async (event) => {
            event.preventDefault();
            const payload = reply ?? {
              client_message_id: crypto.randomUUID(),
              answer: answer.trim(),
            };
            const validation = messageValidation(payload.answer);
            if (validation) {
              setReplyError(validation);
              return;
            }
            setReplyError("");
            setReply(payload);
            if (
              await onUpdate(
                `tasks/${task.id}/respond`,
                payload,
                "Clarification saved. The task continues within its remaining budget.",
              )
            ) {
              setAnswer("");
              setReply(null);
            }
          }}
        >
          <label>
            {task.question}
            <textarea
              rows={3}
              value={answer}
              maxLength={16000}
              required
              disabled={locked || !!reply || task.cancel_requested}
              onChange={(event) => setAnswer(event.target.value)}
            />
          </label>
          <Button
            type="submit"
            disabled={locked || !answer.trim() || task.cancel_requested}
          >
            {reply ? "Retry same clarification" : "Send clarification"}
          </Button>
          {replyError && (
            <p role="alert" className="notice warning">
              {replyError}
            </p>
          )}
        </form>
      )}
      {task.result != null && (
        <Technical label="Task result" value={task.result} />
      )}
      {task.artifacts.length > 0 && (
        <Technical label="Task artifacts" value={task.artifacts} />
      )}
      {!!links.length && (
        <div className="actions">
          {links.map((url) => (
            <Button
              key={url}
              title={url}
              onClick={() => {
                setLinkError("");
                void openNetworkLink(url).catch((cause) =>
                  setLinkError(errorText(cause)),
                );
              }}
            >
              Open {new URL(url).hostname}
            </Button>
          ))}
        </div>
      )}
      {linkError && <p role="alert">{linkError}</p>}
      {!terminal && (
        <Button
          disabled={locked || task.cancel_requested}
          onClick={() =>
            void onUpdate(
              `tasks/${task.id}/cancel`,
              {},
              "Cancellation requested. Status will update after reconciliation.",
            )
          }
        >
          {task.cancel_requested ? "Waiting for cancellation" : "Cancel task"}
        </Button>
      )}
    </article>
  );
}
