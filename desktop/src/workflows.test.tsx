import { beforeEach, describe, expect, it, vi } from "vitest";
import {
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
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in through browser" }),
    );
    expect(service.openService).toHaveBeenCalledWith(
      "login",
      "http://127.0.0.1:4250",
    );
    fireEvent.click(screen.getByRole("button", { name: "Close dialog" }));
    expect(screen.getByRole("button", { name: "Not connected" })).toBeVisible();
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
