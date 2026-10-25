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
import { Memories } from "./Memories";
import { Review } from "./Review";
import { Activity } from "./Activity";
import { allMemories } from "./memories-data";
import * as service from "./service";
import {
  auth,
  detail,
  fixture,
  memory,
  overview,
  proposal,
} from "./testing/fixtures";
vi.mock("./service", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./service")>()),
  connect: vi.fn(),
  disconnect: vi.fn(),
  request: vi.fn(),
  chooseProject: vi.fn(),
  openService: vi.fn(),
  startBrowserLogin: vi.fn(),
  pollBrowserLogin: vi.fn(),
  openBrowserLogin: vi.fn(),
  cancelBrowserLogin: vi.fn(),
  forgetBrowserLogin: vi.fn(),
}));
const api = vi.mocked(service.request);
function mount(node: ReactNode) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      mutations: { retry: false },
    },
  });
  render(<QueryClientProvider client={client}>{node}</QueryClientProvider>);
  return client;
}
beforeEach(() => {
  vi.resetAllMocks();
  localStorage.clear();
  api.mockImplementation(async (path) => fixture(path));
  vi.mocked(service.connect).mockResolvedValue(auth);
  vi.mocked(service.openService).mockResolvedValue(undefined);
});
const browserProgress: service.BrowserLoginProgress = {
  status: "pending",
  request_id: "11111111-1111-4111-8111-111111111111",
  expires_at: "2099-01-01T00:00:00Z",
  auth: null,
};
describe("desktop workflows", () => {
  it("restores the exact endpoint/project and keeps credentials out of storage", async () => {
    localStorage.setItem(
      "codifix.desktop.projects",
      JSON.stringify([
        { endpoint: "http://127.0.0.1:4250", project: "codifix", path: null },
      ]),
    );
    mount(<App />);
    await screen.findByRole("heading", { name: "Pick up where you left off" });
    expect(service.connect).toHaveBeenCalledWith("http://127.0.0.1:4250", null);
    fireEvent.click(screen.getByRole("button", { name: "Memories" }));
    await screen.findByText(detail.canonical_text);
    fireEvent.change(screen.getByRole("textbox", { name: "Search memories" }), {
      target: { value: "nothing matches" },
    });
    expect(
      await screen.findByRole("heading", {
        name: "No memories match these filters",
      }),
    ).toBeVisible();
    expect(localStorage.getItem("codifix.desktop.preferences")).not.toContain(
      "token",
    );
  });
  it("shows connection failures instead of a fabricated empty connected project", async () => {
    vi.mocked(service.connect).mockRejectedValue("Service returned HTTP 401");
    mount(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent("401");
    fireEvent.click(screen.getByRole("button", { name: "Close dialog" }));
    expect(screen.getByRole("button", { name: "Not connected" })).toBeVisible();
  });
  it("completes browser sign-in without device review and keeps only public preferences", async () => {
    vi.mocked(service.connect).mockRejectedValue("Service returned HTTP 401");
    vi.mocked(service.startBrowserLogin).mockResolvedValue(browserProgress);
    vi.mocked(service.openBrowserLogin).mockResolvedValue(undefined);
    vi.mocked(service.pollBrowserLogin).mockResolvedValue({
      ...browserProgress,
      status: "approved",
      auth,
    });
    mount(<App />);
    await screen.findByRole("alert");
    fireEvent.change(screen.getByRole("textbox", { name: "Service URL" }), {
      target: { value: "https://company.example" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    expect(
      await screen.findByText("Finish signing in in your browser"),
    ).toBeVisible();
    expect(service.startBrowserLogin).toHaveBeenCalledWith(
      "https://company.example",
    );
    expect(service.openBrowserLogin).toHaveBeenCalledWith(
      browserProgress.request_id,
    );
    expect(screen.getByRole("textbox", { name: "Service URL" })).toBeDisabled();
    await waitFor(
      () => expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
      { timeout: 4500 },
    );
    expect(screen.getByRole("button", { name: "Connected" })).toBeVisible();
    const saved = localStorage.getItem("codifix.desktop.preferences");
    expect(saved).toContain("https://company.example");
    expect(saved).not.toMatch(/credential|token|fingerprint|request_id/);
  });
  it("shows a quiet sign-in screen for an expired session", async () => {
    vi.mocked(service.connect).mockRejectedValue(
      "Service returned HTTP 401. Refresh before retrying a write.",
    );
    mount(<App />);
    await screen.findByRole("dialog");
    expect(screen.getByRole("button", { name: "Sign in" })).toBeEnabled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(
      screen.queryByText(/fingerprint|User code|Review device request/),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("Connection settings").parentElement,
    ).not.toHaveAttribute("open");
  });
  it("allows a new attempt after browser cancellation", async () => {
    vi.mocked(service.connect).mockRejectedValue("401");
    vi.mocked(service.startBrowserLogin).mockResolvedValue(browserProgress);
    vi.mocked(service.openBrowserLogin).mockResolvedValue(undefined);
    vi.mocked(service.pollBrowserLogin).mockRejectedValue(
      "Sign-in was cancelled in your browser. Try again.",
    );
    mount(<App />);
    await screen.findByRole("alert");
    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    await screen.findByText("Sign-in interrupted");
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() =>
      expect(service.startBrowserLogin).toHaveBeenCalledTimes(2),
    );
  });
  it("discards late approval after the user cancels sign-in", async () => {
    vi.mocked(service.connect).mockRejectedValue("401");
    vi.mocked(service.startBrowserLogin).mockResolvedValue(browserProgress);
    vi.mocked(service.openBrowserLogin).mockResolvedValue(undefined);
    vi.mocked(service.cancelBrowserLogin).mockResolvedValue(undefined);
    let resolve!: (p: service.BrowserLoginProgress) => void;
    vi.mocked(service.pollBrowserLogin).mockImplementation(
      () =>
        new Promise((r) => {
          resolve = r;
        }),
    );
    mount(<App />);
    await screen.findByRole("alert");
    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    await screen.findByText("Finish signing in in your browser");
    await waitFor(() => expect(service.pollBrowserLogin).toHaveBeenCalled(), {
      timeout: 4500,
    });
    fireEvent.click(screen.getByRole("button", { name: "Cancel sign-in" }));
    await waitFor(() => expect(service.cancelBrowserLogin).toHaveBeenCalled());
    await act(async () =>
      resolve({ ...browserProgress, status: "approved", auth }),
    );
    expect(
      screen.queryByRole("button", { name: "Connected" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sign in" })).toBeEnabled();
  });
  it("offers explicitly confirmed local recovery after an unconfirmed cancellation", async () => {
    vi.mocked(service.connect).mockRejectedValue("401");
    vi.mocked(service.cancelBrowserLogin).mockRejectedValue(
      "Sign-out is unconfirmed",
    );
    vi.mocked(service.forgetBrowserLogin).mockResolvedValue(undefined);
    mount(<App />);
    await screen.findByRole("alert");
    fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    await screen.findByText("Sign-out is unconfirmed");
    fireEvent.click(screen.getByText("Recover an unavailable sign-in"));
    const forget = screen.getByRole("button", {
      name: "Forget saved sign-in locally",
    });
    expect(forget).toBeDisabled();
    fireEvent.click(
      screen.getByRole("checkbox", {
        name: "I understand that remote access may remain active.",
      }),
    );
    fireEvent.click(forget);
    await waitFor(() =>
      expect(service.forgetBrowserLogin).toHaveBeenCalledWith(
        "http://127.0.0.1:4250",
      ),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Remote revocation was not confirmed",
    );
  });
  it("reuses saved browser access without opening another approval", async () => {
    vi.mocked(service.connect).mockRejectedValue("401");
    vi.mocked(service.startBrowserLogin).mockResolvedValue({
      ...browserProgress,
      status: "approved",
      auth,
    });
    mount(<App />);
    await screen.findByRole("alert");
    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(service.openBrowserLogin).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Connected" })).toBeVisible();
  });
  it("command palette opens from Ctrl K and handles keyboard selection", async () => {
    mount(<App />);
    await waitFor(() => expect(service.connect).toHaveBeenCalled());
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const input = await screen.findByRole("combobox", {
      name: "Search commands",
    });
    fireEvent.change(input, { target: { value: "Settings" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(
      await screen.findByRole("heading", { name: "Settings" }),
    ).toBeVisible();
  });
  it("loads older memory pages and rejects a mixed-project response", async () => {
    api
      .mockResolvedValueOnce({ project: "codifix", total: 2, items: [memory] })
      .mockResolvedValueOnce({
        project: "codifix",
        total: 2,
        items: [{ ...memory, id: "older" }],
      });
    const result = await allMemories("codifix", new AbortController().signal);
    expect(result.items).toHaveLength(2);
    expect(api).toHaveBeenNthCalledWith(
      2,
      "/v1/projects/codifix/memories?limit=500&offset=1",
    );
    api.mockResolvedValueOnce({
      project: "private-other",
      total: 1,
      items: [memory],
    });
    await expect(
      allMemories("codifix", new AbortController().signal),
    ).rejects.toThrow("different project");
  });
  it("opens a related record outside the loaded list without selecting an unrelated first row", async () => {
    api.mockImplementation(async (path) =>
      path === "/v1/memory/linked"
        ? {
            ...detail,
            id: "linked",
            summary: "Linked old memory",
            canonical_text: "Older knowledge is directly reachable",
          }
        : fixture(path),
    );
    mount(<Memories scope="test" project="codifix" initialId="linked" />);
    expect(
      await screen.findByText("Older knowledge is directly reachable"),
    ).toBeVisible();
  });
  it("reports approval only after validating its receipt", async () => {
    api.mockImplementation(async (path, body) =>
      path.endsWith("/approve")
        ? { project: "wrong", proposal_id: proposal.id, status: "approved" }
        : fixture(path),
    );
    const notify = vi.fn();
    mount(
      <Review
        scope="test"
        project="codifix"
        auth={auth}
        connected
        onBack={() => {}}
        notify={notify}
      />,
    );
    const approve = await screen.findByRole("button", {
      name: "Approve update",
    });
    await waitFor(() => expect(approve).toBeEnabled());
    fireEvent.click(approve);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "receipt did not match",
    );
    expect(notify).not.toHaveBeenCalled();
  });
  it("disables review writes when disconnected or explicitly missing permissions", async () => {
    mount(
      <Review
        scope="test"
        project="codifix"
        auth={{
          ...auth,
          principal: { ...auth.principal, global_permissions: [] },
        }}
        connected={false}
        onBack={() => {}}
        notify={() => {}}
      />,
    );
    expect(
      await screen.findByRole("button", { name: "Approve update" }),
    ).toBeDisabled();
    expect(
      service.canWrite(
        { ...auth, principal: { ...auth.principal, global_permissions: [] } },
        "codifix",
        "memory_curate",
      ),
    ).toBe(false);
  });
  it("keeps retry IDs on uncertain outcomes and rotates them after confirmed success", async () => {
    const id = "55555555-5555-4555-8555-555555555555";
    let attempts = 0;
    const ids: string[] = [];
    const job = {
      id,
      project_id: "p",
      operation_id: "o",
      kind: "refresh_index",
      status: "failed",
      generation: 1,
      attempts: 1,
      last_failure: "Index unavailable",
      created_at: "2026-10-25T10:42:00Z",
      available_at: "2026-10-25T10:42:00Z",
      lease_until: null,
    };
    api.mockImplementation(async (path, body) => {
      if (path.endsWith("/retry")) {
        const value = body as { request_id: string };
        ids.push(value.request_id);
        if (++attempts === 1) throw new Error("Lost response");
        return { request_id: value.request_id, job_id: id, generation: 2 };
      }
      if (path.includes("jobs?"))
        return {
          jobs: [job],
          next_after: null,
          stats: { failed: 1 },
          worker: { enabled: true },
        };
      if (path.endsWith(`/jobs/${id}`))
        return { job, history: [], stages: [], dependencies: [] };
      return fixture(path);
    });
    mount(
      <Activity
        scope="test"
        project="codifix"
        auth={auth}
        connected
        onReview={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Jobs" }));
    fireEvent.click(await screen.findByRole("button", { name: "Details" }));
    const check = await screen.findByRole("checkbox", {
      name: "I confirm this job retry.",
    });
    fireEvent.click(check);
    const retry = screen.getByRole("button", { name: "Confirm retry" });
    fireEvent.click(retry);
    await screen.findByRole("alert");
    fireEvent.click(retry);
    await screen.findByText(
      "Retry accepted. Execution remains subject to current policy.",
    );
    await waitFor(() => expect(check).not.toBeChecked());
    fireEvent.click(check);
    fireEvent.click(retry);
    await waitFor(() => expect(ids).toHaveLength(3));
    expect(ids[0]).toBe(ids[1]);
    expect(ids[2]).not.toBe(ids[1]);
  });
});
