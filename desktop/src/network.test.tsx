import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { App } from "./App";
import { Inbox } from "./Inbox";
import { Network } from "./Network";
import * as service from "./service";
import * as network from "./network-api";
import { auth as originalAuth } from "./testing/fixtures";
import type {
  ConversationDetail,
  IdentityAttempt,
  Specialist,
} from "./network-api";

vi.mock("./service", async (original) => ({
  ...(await original<typeof import("./service")>()),
  request: vi.fn(),
  connect: vi.fn(),
  disconnect: vi.fn(),
  listAreas: vi.fn(),
  resolveArea: vi.fn(),
  selectCustomService: vi.fn(),
}));
vi.mock("./network-api", async (original) => ({
  ...(await original<typeof import("./network-api")>()),
  openIdentityAttempt: vi.fn(),
  openNetworkLink: vi.fn(),
}));
const api = vi.mocked(service.request);
const ids = {
  human: "00000000-0000-4000-8000-000000000001",
  instance: "00000000-0000-4000-8000-000000000002",
  conversation: "00000000-0000-4000-8000-000000000003",
  lead: "00000000-0000-4000-8000-000000000004",
  other: "00000000-0000-4000-8000-000000000005",
  member: "00000000-0000-4000-8000-000000000006",
  message: "00000000-0000-4000-8000-000000000007",
  task: "00000000-0000-4000-8000-000000000008",
};
const auth: service.AuthMeResponse = {
  ...originalAuth,
  mode: "multi_user",
  principal: { ...originalAuth.principal, id: ids.human, kind: "human_oidc" },
};
const lead: Specialist = {
  id: ids.lead,
  instance_id: ids.instance,
  specialist_id: ids.lead,
  display_name: "Planning specialist",
  summary: "Turns agreed plans into trackable work.",
  capabilities: ["conversation_turn", "plan_prepare"],
  enrolled: true,
  available: true,
  grant_id: ids.lead,
  remote: false,
};
const other: Specialist = {
  ...lead,
  id: ids.other,
  specialist_id: ids.other,
  display_name: "Documentation specialist",
  remote: true,
};
const status = {
  enabled: true,
  home_instance_id: ids.instance,
  identity: null,
  identity_linking_available: true,
};
const timestamp = "2026-11-01T12:00:00Z";
const base: ConversationDetail = {
  conversation: {
    id: ids.conversation,
    title: "Release roadmap",
    lead_member_id: ids.member,
    paused: false,
    revision: 1,
    created_at: timestamp,
    updated_at: timestamp,
    unread_count: 0,
  },
  members: [
    {
      id: ids.member,
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
      id: ids.message,
      conversation_id: ids.conversation,
      sequence: 1,
      role: "human",
      member_id: null,
      body: "The launch needs an accessible guide.",
      format: "text",
      artifact: null,
      created_at: timestamp,
      delivery: "confirmed",
      root_id: ids.message,
      recipient_member_ids: [],
    },
  ],
  tasks: [],
};
let detail: ConversationDetail;
const props = {
  scope: "home:owner",
  auth,
  connected: true,
  onConnect: vi.fn(),
};
function mount(node: ReactNode) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      mutations: { retry: false },
    },
  });
  return {
    ...render(
      <QueryClientProvider client={client}>{node}</QueryClientProvider>,
    ),
    client,
  };
}
function defaults(path: string) {
  if (path === "/v1/auth/me") return auth;
  if (path === "/v1/network/status") return status;
  if (path === "/v1/network/specialists") return [lead, other];
  if (path === "/v1/inbox/conversations") return [detail.conversation];
  if (path.startsWith(`/v1/inbox/conversations/${ids.conversation}`))
    return detail;
  throw new Error(`Unexpected test request: ${path}`);
}
beforeEach(() => {
  vi.resetAllMocks();
  localStorage.clear();
  detail = structuredClone(base);
  api.mockImplementation(async (path) => defaults(path));
  vi.mocked(service.connect).mockResolvedValue(auth);
  vi.mocked(service.disconnect).mockResolvedValue(undefined);
  vi.mocked(service.listAreas).mockResolvedValue([]);
  vi.mocked(service.resolveArea).mockResolvedValue(null);
  vi.mocked(service.selectCustomService).mockResolvedValue(undefined);
  vi.mocked(network.openIdentityAttempt).mockResolvedValue(undefined);
});

describe("private Network and Inbox", () => {
  it("opens Inbox and Network without selecting a project", async () => {
    mount(<App />);
    await screen.findByRole("button", { name: "Connected" });
    fireEvent.click(screen.getByRole("button", { name: "Inbox" }));
    expect(
      await screen.findByRole("heading", { name: "Release roadmap" }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Choose a project" }),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Network" }));
    expect(
      await screen.findByRole("heading", { name: "Your AT identity" }),
    ).toBeVisible();
    expect(screen.getByText("Your specialists")).toBeVisible();
  });
  it("reports a disabled or older service without enabling private actions", async () => {
    api.mockImplementation(async (path) => {
      if (path === "/v1/network/status")
        throw "Service returned HTTP 404. Refresh before retrying a write.";
      return defaults(path);
    });
    mount(<Inbox {...props} onNetwork={vi.fn()} />);
    expect(
      await screen.findByRole("heading", {
        name: "Network and Inbox are not enabled",
      }),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "New conversation" }),
    ).not.toBeInTheDocument();
    expect(api).not.toHaveBeenCalledWith("/v1/network/specialists");
  });
  it("requires a human identity rather than treating an operator service token as a person", async () => {
    mount(
      <Inbox
        {...props}
        auth={{
          ...auth,
          principal: { ...auth.principal, kind: "service_token" },
        }}
        onNetwork={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("heading", { name: "Sign in as yourself" }),
    ).toBeVisible();
    expect(api).not.toHaveBeenCalled();
  });
  it("opens account linking by attempt ID and polls for verified completion", async () => {
    const pending: IdentityAttempt = {
      attempt_id: ids.task,
      status: "pending",
      authorization_url: "https://pds.example/authorize",
      expires_at: "2099-01-01T00:00:00Z",
    };
    let linked = false;
    api.mockImplementation(async (path) => {
      if (path === "/v1/network/status")
        return {
          ...status,
          identity: linked
            ? {
                did: "did:plc:verified",
                handle: "me.example.com",
                linked_at: timestamp,
              }
            : null,
        };
      if (path === "/v1/network/identity/link") return pending;
      if (path === `/v1/network/identity/link/${ids.task}`) {
        linked = true;
        return { ...pending, status: "complete", authorization_url: null };
      }
      return defaults(path);
    });
    mount(<Network {...props} onInbox={vi.fn()} />);
    fireEvent.change(
      await screen.findByRole("textbox", { name: "AT handle or DID" }),
      { target: { value: "me.example.com" } },
    );
    fireEvent.click(screen.getByRole("button", { name: "Link AT identity" }));
    await screen.findByText("Finish linking in your browser");
    expect(network.openIdentityAttempt).toHaveBeenCalledWith(ids.task);
    expect(
      await screen.findByText("me.example.com", {}, { timeout: 3500 }),
    ).toBeVisible();
    expect(localStorage.length).toBe(0);
    expect(
      screen.getByText(
        "AT identity linked. Your existing permissions are unchanged.",
      ),
    ).toBeVisible();
  });
  it("retries an uncertain message with the exact original ID and payload", async () => {
    let calls = 0;
    api.mockImplementation(async (path, body) => {
      if (path.endsWith("/messages")) {
        calls++;
        if (calls === 1) throw new Error("The service request timed out.");
        const message = body as network.SendMessage;
        detail = {
          ...detail,
          messages: [
            ...detail.messages,
            {
              ...detail.messages[0],
              id: message.client_message_id,
              body: message.body,
              sequence: 2,
              root_id: message.client_message_id,
              recipient_member_ids: message.recipient_member_ids,
            },
          ],
        };
        return detail;
      }
      return defaults(path);
    });
    mount(<Inbox {...props} onNetwork={vi.fn()} />);
    fireEvent.change(await screen.findByRole("textbox", { name: "Message" }), {
      target: { value: "Plan the documentation milestone." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send message" }));
    expect(await screen.findByText("Delivery unconfirmed")).toBeVisible();
    expect(screen.getByRole("textbox", { name: "Message" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Retry same message" }));
    await waitFor(() =>
      expect(
        screen.queryByText("Delivery unconfirmed"),
      ).not.toBeInTheDocument(),
    );
    const writes = api.mock.calls.filter(([path]) =>
      path.endsWith("/messages"),
    );
    expect(writes).toHaveLength(2);
    expect(writes[0][1]).toEqual(writes[1][1]);
    expect(
      screen.getAllByText("Plan the documentation milestone."),
    ).toHaveLength(1);
    expect(screen.getByRole("textbox", { name: "Message" })).toHaveValue("");
  });
  it("requires review of summary and selected messages before inviting a specialist", async () => {
    mount(<Inbox {...props} onNetwork={vi.fn()} />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Add specialist" }),
    );
    const dialog = screen.getByRole("dialog");
    const summary = within(dialog).getByRole("textbox", {
      name: "Summary to share",
    });
    fireEvent.change(summary, {
      target: { value: "Prepare the launch guide." },
    });
    const submit = within(dialog).getByRole("button", {
      name: "Confirm context and add specialist",
    });
    expect(submit).toBeDisabled();
    fireEvent.click(
      within(dialog).getByRole("checkbox", {
        name: "The launch needs an accessible guide.",
      }),
    );
    fireEvent.click(
      within(dialog).getByRole("checkbox", {
        name: /I reviewed this summary and 1 selected messages/,
      }),
    );
    expect(submit).toBeEnabled();
    fireEvent.change(summary, {
      target: { value: "Prepare the guide with accessibility criteria." },
    });
    expect(submit).toBeDisabled();
    fireEvent.click(
      within(dialog).getByRole("checkbox", { name: /I reviewed this summary/ }),
    );
    fireEvent.click(submit);
    await waitFor(() =>
      expect(api).toHaveBeenCalledWith(
        network.inboxPath(ids.conversation, "members"),
        expect.objectContaining({
          specialist_id: ids.other,
          summary: "Prepare the guide with accessibility criteria.",
          selected_message_ids: [ids.message],
          confirmed: true,
        }),
      ),
    );
  });
  it("pauses ordinary messages when lead is unavailable and permits explicit available responders", async () => {
    detail.members[0].specialist.available = false;
    detail.members.push({
      ...detail.members[0],
      id: ids.other,
      specialist: other,
    });
    mount(<Inbox {...props} onNetwork={vi.fn()} />);
    fireEvent.change(await screen.findByRole("textbox", { name: "Message" }), {
      target: { value: "Write the outline." },
    });
    expect(screen.getByRole("button", { name: "Send message" })).toBeDisabled();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "@Documentation specialist" }),
    );
    expect(screen.getByRole("button", { name: "Send message" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "Send message" }));
    await waitFor(() =>
      expect(api).toHaveBeenCalledWith(
        network.inboxPath(ids.conversation, "messages"),
        expect.objectContaining({ recipient_member_ids: [ids.other] }),
      ),
    );
  });
  it("keeps remote cancellation pending until the server confirms termination", async () => {
    detail.tasks = [
      {
        id: ids.task,
        member_id: ids.member,
        status: "working",
        objective: "Create roadmap milestones",
        question: null,
        result: null,
        artifacts: [],
        cancel_requested: false,
        root_id: ids.message,
      },
    ];
    api.mockImplementation(async (path) => {
      if (path.endsWith("/cancel"))
        detail = {
          ...detail,
          tasks: detail.tasks.map((task) => ({
            ...task,
            cancel_requested: true,
          })),
        };
      return defaults(path);
    });
    mount(<Inbox {...props} onNetwork={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Cancel task" }));
    expect(await screen.findByText("Cancellation pending")).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Waiting for cancellation" }),
    ).toBeDisabled();
  });
  it("does not turn an OpenBao recovery question into an unseal-share input", async () => {
    detail.members[0].specialist.capabilities = ["openbao_recover"];
    detail.tasks = [
      {
        id: ids.task,
        member_id: ids.member,
        status: "input_required",
        objective: "Recover OpenBao",
        question: "Paste an unseal key",
        result: null,
        artifacts: [],
        cancel_requested: false,
        root_id: ids.message,
      },
    ];
    mount(<Inbox {...props} onNetwork={vi.fn()} />);
    expect(
      await screen.findByText(
        /Recovery runs through the protected OpenBao controller/,
      ),
    ).toBeVisible();
    expect(
      screen.queryByLabelText("Paste an unseal key"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Paste an unseal key")).not.toBeInTheDocument();
  });
  it("discards a previous account's late conversation response", async () => {
    let resolve!: (value: ConversationDetail) => void;
    api.mockImplementation(async (path) => {
      if (path === network.inboxPath(ids.conversation))
        return new Promise<ConversationDetail>((done) => {
          resolve = done;
        });
      return defaults(path);
    });
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, gcTime: 0 } },
    });
    const { rerender } = render(
      <QueryClientProvider client={client}>
        <Inbox {...props} onNetwork={vi.fn()} />
      </QueryClientProvider>,
    );
    await waitFor(() => expect(resolve).toBeTypeOf("function"));
    api.mockImplementation(async (path) =>
      path === "/v1/inbox/conversations" ? [] : defaults(path),
    );
    rerender(
      <QueryClientProvider client={client}>
        <Inbox
          {...props}
          scope="other:owner"
          auth={{ ...auth, principal: { ...auth.principal, id: ids.other } }}
          onNetwork={vi.fn()}
        />
      </QueryClientProvider>,
    );
    await screen.findByRole("heading", { name: "Start a conversation" });
    await act(async () => resolve(detail));
    expect(
      screen.queryByText("The launch needs an accessible guide."),
    ).not.toBeInTheDocument();
  });
});
