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
  listAreas: vi.fn(),
  resolveArea: vi.fn(),
  selectArea: vi.fn(),
  selectCustomService: vi.fn(),
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
  vi.mocked(service.listAreas).mockResolvedValue([]);
  vi.mocked(service.resolveArea).mockResolvedValue(null);
  vi.mocked(service.selectArea).mockResolvedValue(null);
  vi.mocked(service.disconnect).mockResolvedValue(undefined);
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

const workshopArea: service.AreaBinding = {
  name: "workshop",
  instance_id: "11111111-1111-4111-8111-111111111111",
  endpoint: "https://workshop.example",
};
const homelabArea: service.AreaBinding = {
  name: "homelab",
  instance_id: "22222222-2222-4222-8222-222222222222",
  endpoint: "https://homelab.example",
};
function configuredAreas() {
  vi.mocked(service.listAreas).mockResolvedValue(
    [workshopArea, homelabArea].map((binding) => ({
      name: binding.name,
      instance_id: binding.instance_id,
      binding,
      error: null,
    })),
  );
  vi.mocked(service.selectArea).mockImplementation(async (name) =>
    name === "homelab" ? homelabArea : workshopArea,
  );
  localStorage.setItem(
    "codifix.desktop.preferences",
    JSON.stringify({
      endpoint: workshopArea.endpoint,
      connectionName: "workshop",
      area: workshopArea,
    }),
  );
}
describe("desktop area routing", () => {
  it("keeps equal project slugs and cached data scoped to origin and area UUID", async () => {
    configuredAreas();
    localStorage.setItem(
      "codifix.desktop.projects",
      JSON.stringify([
        {
          project: "codifix",
          endpoint: workshopArea.endpoint,
          areaId: workshopArea.instance_id,
          path: null,
        },
        {
          project: "codifix",
          endpoint: homelabArea.endpoint,
          areaId: homelabArea.instance_id,
          path: null,
        },
      ]),
    );
    const client = mount(<App />);
    await screen.findByRole("heading", { name: "Pick up where you left off" });
    client.setQueryData(["workshop-private"], { private: true });
    fireEvent.change(screen.getByRole("combobox", { name: "Area" }), {
      target: { value: "homelab" },
    });
    await waitFor(() =>
      expect(service.connect).toHaveBeenLastCalledWith(
        homelabArea.endpoint,
        null,
      ),
    );
    expect(client.getQueryData(["workshop-private"])).toBeUndefined();
    expect(
      screen.queryByRole("heading", { name: "Pick up where you left off" }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Choose a project" }));
    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: /codifix/,
      }),
    );
    await screen.findByRole("heading", { name: "Pick up where you left off" });
    const saved = JSON.parse(localStorage.getItem("codifix.desktop.projects")!);
    expect(
      saved.filter((r: { project: string }) => r.project === "codifix"),
    ).toHaveLength(2);
    expect(saved[0].areaId).toBe(homelabArea.instance_id);
  });
  it("resolves a folder before requests, switches origin, and restores it after browser login", async () => {
    configuredAreas();
    vi.mocked(service.chooseProject).mockResolvedValue({
      path: "/repos/infra",
      slug: "homelab",
    });
    vi.mocked(service.resolveArea).mockResolvedValue(homelabArea);
    vi.mocked(service.connect).mockImplementation(async (endpoint) =>
      endpoint === homelabArea.endpoint
        ? { ...auth, mode: "multi_user", authenticated: false }
        : auth,
    );
    vi.mocked(service.startBrowserLogin).mockResolvedValue({
      ...browserProgress,
      status: "approved",
      auth,
    });
    api.mockImplementation(async (path) =>
      path === "/v1/projects/homelab/overview"
        ? { ...overview, project: "homelab" }
        : fixture(path),
    );
    const client = mount(<App />);
    await waitFor(() =>
      expect(service.connect).toHaveBeenCalledWith(workshopArea.endpoint, null),
    );
    client.setQueryData(["workshop-private"], "cached");
    fireEvent.click(screen.getByRole("button", { name: "Choose a project" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Open project folder" }),
    );
    await screen.findByRole("dialog", { name: "Sign in to Codifix" });
    expect(service.resolveArea).toHaveBeenCalledWith(
      null,
      "/repos/infra",
      "homelab",
    );
    expect(service.selectArea).toHaveBeenLastCalledWith(
      "homelab",
      "/repos/infra",
      "homelab",
    );
    expect(api).not.toHaveBeenCalledWith("/v1/projects/homelab/overview");
    expect(client.getQueryData(["workshop-private"])).toBeUndefined();
    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    await screen.findByRole("heading", { name: "Pick up where you left off" });
    expect(service.startBrowserLogin).toHaveBeenCalledWith(
      homelabArea.endpoint,
    );
    expect(
      JSON.parse(localStorage.getItem("codifix.desktop.projects")!)[0],
    ).toMatchObject({
      project: "homelab",
      path: "/repos/infra",
      endpoint: homelabArea.endpoint,
      areaId: homelabArea.instance_id,
    });
  });
  it("fails closed for an unavailable saved area without connecting to Workshop", async () => {
    configuredAreas();
    localStorage.setItem(
      "codifix.desktop.preferences",
      JSON.stringify({
        endpoint: homelabArea.endpoint,
        connectionName: "homelab",
        area: homelabArea,
      }),
    );
    vi.mocked(service.selectArea).mockRejectedValue(
      "area homelab config is unavailable; no fallback was attempted",
    );
    mount(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "homelab config is unavailable",
    );
    expect(service.connect).not.toHaveBeenCalled();
  });
  it("discards a previous area's delayed project response after switching", async () => {
    configuredAreas();
    localStorage.setItem(
      "codifix.desktop.projects",
      JSON.stringify([
        {
          project: "codifix",
          endpoint: workshopArea.endpoint,
          areaId: workshopArea.instance_id,
          path: null,
        },
      ]),
    );
    let finish!: (value: typeof overview) => void;
    api.mockImplementation((path) =>
      path === "/v1/projects/codifix/overview"
        ? new Promise((resolve) => {
            finish = resolve;
          })
        : Promise.resolve(fixture(path)),
    );
    mount(<App />);
    await waitFor(() => expect(finish).toBeTypeOf("function"));
    fireEvent.change(screen.getByRole("combobox", { name: "Area" }), {
      target: { value: "homelab" },
    });
    await waitFor(() =>
      expect(service.connect).toHaveBeenLastCalledWith(
        homelabArea.endpoint,
        null,
      ),
    );
    await act(async () => finish(overview));
    expect(
      screen.queryByRole("heading", { name: "Pick up where you left off" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Area" })).toHaveValue(
      "homelab",
    );
  });
});

describe("desktop area upgrade recovery", () => {
  it("migrates legacy recents by exact origin and never by project name", async () => {
    configuredAreas();
    localStorage.setItem(
      "codifix.desktop.preferences",
      JSON.stringify({
        endpoint: homelabArea.endpoint,
        connectionName: "Homelab",
      }),
    );
    localStorage.setItem(
      "codifix.desktop.projects",
      JSON.stringify([
        { project: "homelab", endpoint: workshopArea.endpoint, path: null },
        { project: "homelab", endpoint: homelabArea.endpoint, path: null },
      ]),
    );
    api.mockImplementation(async (path) =>
      path === "/v1/projects/homelab/overview"
        ? { ...overview, project: "homelab" }
        : fixture(path),
    );
    mount(<App />);
    await screen.findByRole("heading", { name: "Pick up where you left off" });
    expect(service.connect).toHaveBeenCalledWith(homelabArea.endpoint, null);
    const recents = JSON.parse(
      localStorage.getItem("codifix.desktop.projects")!,
    );
    expect(
      recents.find(
        (r: { endpoint: string }) => r.endpoint === workshopArea.endpoint,
      ).areaId,
    ).toBe(workshopArea.instance_id);
    expect(
      recents.find(
        (r: { endpoint: string }) => r.endpoint === homelabArea.endpoint,
      ).areaId,
    ).toBe(homelabArea.instance_id);
    expect(service.resolveArea).not.toHaveBeenCalled();
  });
  it("clears the previous area when a folder's required area is unavailable", async () => {
    configuredAreas();
    vi.mocked(service.chooseProject).mockResolvedValue({
      path: "/repos/infra",
      slug: "homelab",
    });
    vi.mocked(service.resolveArea).mockRejectedValue(
      "area homelab config is unavailable; no fallback was attempted",
    );
    const client = mount(<App />);
    await waitFor(() =>
      expect(service.connect).toHaveBeenCalledWith(workshopArea.endpoint, null),
    );
    client.setQueryData(["old-area"], "private");
    fireEvent.click(screen.getByRole("button", { name: "Choose a project" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Open project folder" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "homelab config is unavailable",
    );
    expect(client.getQueryData(["old-area"])).toBeUndefined();
    expect(api).not.toHaveBeenCalledWith("/v1/projects/homelab/overview");
    expect(service.disconnect).toHaveBeenCalled();
  });
});

describe("desktop stale folder actions", () => {
  it("ignores an earlier folder route that finishes after a newer area selection", async () => {
    configuredAreas();
    vi.mocked(service.chooseProject).mockResolvedValue({
      path: "/repos/infra",
      slug: "homelab",
    });
    let finish!: (area: service.AreaBinding) => void;
    vi.mocked(service.resolveArea).mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    mount(<App />);
    await waitFor(() =>
      expect(service.connect).toHaveBeenCalledWith(workshopArea.endpoint, null),
    );
    fireEvent.click(screen.getByRole("button", { name: "Choose a project" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Open project folder" }),
    );
    await waitFor(() => expect(finish).toBeTypeOf("function"));
    fireEvent.change(
      screen.getByRole("combobox", { name: "Area", hidden: true }),
      { target: { value: "custom" } },
    );
    await waitFor(() => expect(service.selectCustomService).toHaveBeenCalled());
    await act(async () => finish(homelabArea));
    expect(service.selectArea).not.toHaveBeenCalledWith(
      "homelab",
      "/repos/infra",
      "homelab",
    );
    expect(service.connect).not.toHaveBeenCalledWith(
      homelabArea.endpoint,
      null,
    );
    expect(screen.getByRole("combobox", { name: "Area" })).toHaveValue(
      "custom",
    );
  });
  it("re-resolves a legacy saved folder before restoring its project from another area", async () => {
    configuredAreas();
    localStorage.setItem(
      "codifix.desktop.projects",
      JSON.stringify([
        {
          project: "homelab",
          endpoint: workshopArea.endpoint,
          path: "/repos/infra",
        },
      ]),
    );
    vi.mocked(service.resolveArea).mockResolvedValue(homelabArea);
    api.mockImplementation(async (path) =>
      path === "/v1/projects/homelab/overview"
        ? { ...overview, project: "homelab" }
        : fixture(path),
    );
    mount(<App />);
    await screen.findByRole("heading", { name: "Pick up where you left off" });
    expect(service.connect).toHaveBeenLastCalledWith(
      homelabArea.endpoint,
      null,
    );
    const overviewCall = api.mock.calls.findIndex(
      ([path]) => path === "/v1/projects/homelab/overview",
    );
    expect(api.mock.invocationCallOrder[overviewCall]).toBeGreaterThan(
      vi.mocked(service.connect).mock.invocationCallOrder[1],
    );
    expect(
      JSON.parse(localStorage.getItem("codifix.desktop.projects")!)[0].areaId,
    ).toBe(homelabArea.instance_id);
  });
});
