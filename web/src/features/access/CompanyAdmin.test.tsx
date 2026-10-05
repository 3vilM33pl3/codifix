import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { onboardingRequest, OnboardingRequestError } from "../../api";
import { UsersPanel } from "./UsersPanel";
import { ProjectsPanel } from "./ProjectsPanel";
import { DevicesPanel } from "./DevicesPanel";
import { ProviderOperationsPanel } from "./ProviderOperationsPanel";
import type { EffectiveUserAccess, ManagedDevice, ManagedUser, OnboardingManifest } from "./types";

vi.mock("../../api", async (importOriginal) => ({ onboardingRequest: vi.fn(), OnboardingRequestError: (await importOriginal<typeof import("../../api")>()).OnboardingRequestError }));
afterEach(cleanup);
beforeEach(() => { vi.mocked(onboardingRequest).mockReset(); });
const user: ManagedUser = {
  principal: { id: "user-id", kind: "human_oidc", display_name: "Ada", email: "ada@example.test", issuer: "https://id.example.test", subject: "verified-subject", groups: ["workshop"], projects: [] },
  disabled_at: null, created_at: "2026-10-05T00:00:00Z",
};
const access: EffectiveUserAccess = {
  principal_id: "user-id", disabled: false, global_permissions: [],
  projects: [{ project: "memory", permissions: ["memory_read"], grants: [{ source: "oidc_group", role: "reader", group: "workshop" }] }],
  group_rules: [{ group: "workshop", role: "reader", global: false, project: "memory", matched: true }],
};
const manifest: OnboardingManifest = {
  project_id: "00000000-0000-0000-0000-000000000001", project: "memory", area: "trust", owner_instance_id: "00000000-0000-0000-0000-000000000002",
  repository_url: "https://github.com/example/memory", default_branch: "main", offline_allowed: false,
  tools: [{ name: "memory", version: "2.1.0", url: "https://example.test/memory.tar.gz", sha256: "a".repeat(64) }],
  destinations: [{ name: "Local model", provider_id: "provider-1", destination_id: "model-1" }],
  revision: 3, reviewed_by: "admin-id", reviewed_at: "2026-10-05T00:00:00Z",
};
const device: ManagedDevice = { id: "device-id", principal_id: "user-id", name: "Ada's Mac", platform: "macos", created_at: "2026-10-05T00:00:00Z", last_seen_at: null, revoked_at: null };

function usersApi(enabled = false) {
  vi.mocked(onboardingRequest).mockImplementation(async (suffix) => {
    if (suffix === "/users") return { users: [user] };
    if (suffix === "/capabilities") return { schema_version: 1, idp_management_enabled: enabled, invitation_enabled: enabled, idp_provider: enabled ? "authentik" : null };
    if (suffix === "/idp/operations") return { operations: [] };
    if (suffix.startsWith("/idp/operations/")) return { id: suffix.split("/").at(-1), operation: "invite", outcome: "pending", provider_user_id: null, invitation_id: null };
    if (suffix.endsWith("/access")) return access;
    return user;
  });
}

describe("company users", () => {
  it("shows verified identity and effective group grants while provider operations are opt-in", async () => {
    usersApi();
    render(<UsersPanel readOnly={false} onChanged={vi.fn()} />);
    expect(await screen.findByText(/verified-subject/)).toBeInTheDocument();
    expect(await screen.findByText("oidc_group: reader (workshop)")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create provider user" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Create provider invitation" })).toBeDisabled();
  });
  it("registers a stable pre-login identity with the exact issuer and subject", async () => {
    usersApi(); const changed = vi.fn();
    render(<UsersPanel readOnly={false} onChanged={changed} />);
    await screen.findByText(/verified-subject/);
    fireEvent.change(screen.getByLabelText("Verified OIDC issuer"), { target: { value: "https://id.example.test" } });
    fireEvent.change(screen.getByLabelText("Verified OIDC subject"), { target: { value: "new-subject" } });
    fireEvent.change(screen.getByLabelText("User display name"), { target: { value: "New User" } });
    fireEvent.click(screen.getByRole("button", { name: "Register identity" }));
    await waitFor(() => expect(onboardingRequest).toHaveBeenCalledWith("/users", { issuer: "https://id.example.test", subject: "new-subject", display_name: "New User" }));
    await waitFor(() => expect(changed).toHaveBeenCalledOnce());
  });
  it("does not permit user or provider mutations in read-only mode", async () => {
    usersApi(true);
    render(<UsersPanel readOnly onChanged={vi.fn()} />);
    await screen.findByText(/verified-subject/);
    fireEvent.change(screen.getByLabelText("Provider username"), { target: { value: "ada" } });
    fireEvent.change(screen.getByLabelText("Provider display name"), { target: { value: "Ada" } });
    fireEvent.change(screen.getByLabelText("Provider email"), { target: { value: "ada@example.test" } });
    for (const name of ["Register identity", "Disable Memory user", "Create provider user", "Create provider invitation", "Disable provider user"]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
  });
  it("refreshes central effective access after a membership change", async () => {
    usersApi();
    const { rerender } = render(<UsersPanel readOnly={false} onChanged={vi.fn()} accessVersion={0} />);
    await screen.findByText("oidc_group: reader (workshop)");
    const reads = vi.mocked(onboardingRequest).mock.calls.filter((call) => call[0].endsWith("/access")).length;
    rerender(<UsersPanel readOnly={false} onChanged={vi.fn()} accessVersion={1} />);
    await waitFor(() => expect(vi.mocked(onboardingRequest).mock.calls.filter((call) => call[0].endsWith("/access"))).toHaveLength(reads + 1));
  });
  it("reports provider pending outcome without granting Memory access", async () => {
    usersApi(true);
    const original = vi.mocked(onboardingRequest).getMockImplementation()!;
    vi.mocked(onboardingRequest).mockImplementation(async (...args) => args[0] === "/users/invite"
      ? { id: "op-1", operation: "invite", outcome: "pending", provider_user_id: null, invitation_id: null }
      : original(...args));
    render(<UsersPanel readOnly={false} onChanged={vi.fn()} />);
    await screen.findByText(/verified-subject/);
    fireEvent.change(screen.getByLabelText("Provider username"), { target: { value: "ada" } });
    fireEvent.change(screen.getByLabelText("Provider display name"), { target: { value: "Ada" } });
    fireEvent.change(screen.getByLabelText("Provider email"), { target: { value: "ada@example.test" } });
    fireEvent.click(screen.getByRole("button", { name: "Create provider invitation" }));
    expect(await screen.findByText(/Provider invitation creation: pending. Operation op-1/)).toBeInTheDocument();
    expect(vi.mocked(onboardingRequest).mock.calls.filter((call) => call[1] !== undefined).map((call) => call[0])).toEqual(["/users/invite"]);
  });

  it("shows an uncertain successful HTTP response and never automatically resubmits", async () => {
    usersApi(true);
    const original = vi.mocked(onboardingRequest).getMockImplementation()!;
    vi.mocked(onboardingRequest).mockImplementation(async (...args) => args[0] === "/users/invite"
      ? { id: "op-uncertain", operation: "invite", outcome: "uncertain", provider_user_id: null, invitation_id: null }
      : original(...args));
    render(<UsersPanel readOnly={false} onChanged={vi.fn()} />);
    await screen.findByText(/verified-subject/);
    fireEvent.change(screen.getByLabelText("Provider username"), { target: { value: "ada" } });
    fireEvent.change(screen.getByLabelText("Provider display name"), { target: { value: "Ada" } });
    fireEvent.change(screen.getByLabelText("Provider email"), { target: { value: "ada@example.test" } });
    fireEvent.click(screen.getByRole("button", { name: "Create provider invitation" }));
    expect(await screen.findByText(/Provider invitation creation: uncertain. Operation op-uncertain/)).toHaveTextContent("duplicate an external effect");
    fireEvent.click(screen.getByRole("button", { name: "Refresh users" }));
    await waitFor(() => expect(vi.mocked(onboardingRequest).mock.calls.filter((call) => call[0] === "/idp/operations").length).toBeGreaterThan(1));
    expect(vi.mocked(onboardingRequest).mock.calls.filter((call) => call[1] !== undefined)).toHaveLength(1);
  });

  it("preserves a safe operation reference after lost 503 confirmation and restores durable metadata after reload", async () => {
    const operationId = "11111111-1111-4111-8111-111111111111";
    const operation = { id: operationId, operation: "invite", outcome: "pending", invitation_id: "22222222-2222-4222-8222-222222222222", provider_user_id: null };
    let sent = false;
    usersApi(true);
    const original = vi.mocked(onboardingRequest).getMockImplementation()!;
    vi.mocked(onboardingRequest).mockImplementation(async (...args) => {
      if (args[0] === "/users/invite") { sent = true; throw new OnboardingRequestError(`Provider action unconfirmed. Operation ${operationId}. Inspect Authentik before resubmitting.`, operationId); }
      if (args[0] === "/idp/operations") return { operations: sent ? [operation] : [] };
      if (args[0] === `/idp/operations/${operationId}`) return operation;
      return original(...args);
    });
    const view = render(<UsersPanel readOnly={false} onChanged={vi.fn()} />);
    await screen.findByText(/verified-subject/);
    fireEvent.change(screen.getByLabelText("Provider username"), { target: { value: "ada" } });
    fireEvent.change(screen.getByLabelText("Provider display name"), { target: { value: "Ada" } });
    fireEvent.change(screen.getByLabelText("Provider email"), { target: { value: "ada@example.test" } });
    fireEvent.click(screen.getByRole("button", { name: "Create provider invitation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(operationId);
    expect(await screen.findByRole("heading", { name: `Operation ${operationId}` })).toBeInTheDocument();
    expect(await screen.findByText(operation.invitation_id)).toBeInTheDocument();
    view.unmount();
    render(<UsersPanel readOnly={false} onChanged={vi.fn()} />);
    expect(await screen.findByText(operationId)).toBeInTheDocument();
    expect(await screen.findByText(operation.invitation_id)).toBeInTheDocument();
    expect(screen.getByText("pending — inspect before resubmitting")).toBeInTheDocument();
    expect(vi.mocked(onboardingRequest).mock.calls.filter((call) => call[1] !== undefined)).toHaveLength(1);
  });
});

describe("project manifest reviews", () => {
  beforeEach(() => { vi.mocked(onboardingRequest).mockResolvedValue({ schema_version: 1, projects: [manifest] }); });
  it("forces Trust remote-only and sends only the exact confirmed review with revision fencing", async () => {
    vi.mocked(onboardingRequest).mockImplementation(async (_suffix, body) => body ? { ...manifest, revision: 4 } : { schema_version: 1, projects: [manifest] });
    render(<ProjectsPanel readOnly={false} />);
    fireEvent.click(await screen.findByRole("button", { name: "Review memory" }));
    expect(screen.getByLabelText("Allow offline project data")).toBeDisabled();
    expect(screen.getByLabelText("Allow offline project data")).not.toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "Review manifest changes" }));
    expect(screen.getByRole("button", { name: "Save reviewed manifest" })).toBeDisabled();
    expect(screen.getByLabelText("Repository URL")).toBeDisabled();
    fireEvent.click(screen.getByLabelText("I reviewed this project identity, area, owner, repository, tools and destinations."));
    fireEvent.click(screen.getByRole("button", { name: "Save reviewed manifest" }));
    const { project_id, project: _project, revision, reviewed_by: _reviewedBy, reviewed_at: _reviewedAt, ...entry } = manifest;
    await waitFor(() => expect(onboardingRequest).toHaveBeenCalledWith(`/projects/${project_id}`, { expected_revision: revision, entry }));
    expect(await screen.findByText("Manifest for memory saved at revision 4.")).toBeInTheDocument();
  });
  it("clears offline permission when changing an area to Trust", async () => {
    render(<ProjectsPanel readOnly={false} />);
    await screen.findByRole("button", { name: "Review memory" });
    fireEvent.click(screen.getByLabelText("Allow offline project data"));
    expect(screen.getByLabelText("Allow offline project data")).toBeChecked();
    fireEvent.change(screen.getByLabelText("Memory area"), { target: { value: "trust" } });
    expect(screen.getByLabelText("Allow offline project data")).not.toBeChecked();
    expect(screen.getByLabelText("Allow offline project data")).toBeDisabled();
  });
  it("disables saving on read-only services and surfaces inventory errors", async () => {
    render(<ProjectsPanel readOnly />);
    fireEvent.click(await screen.findByRole("button", { name: "Review memory" }));
    expect(screen.getByRole("button", { name: "Review manifest changes" })).toBeDisabled();
    cleanup();
    vi.mocked(onboardingRequest).mockRejectedValue(new Error("Access denied"));
    render(<ProjectsPanel readOnly={false} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Access denied");
  });
  it("blocks unpinned tool input before sending any write", async () => {
    render(<ProjectsPanel readOnly={false} />);
    fireEvent.click(await screen.findByRole("button", { name: "Review memory" }));
    fireEvent.change(screen.getByLabelText("Pinned tools (JSON array)"), { target: { value: '[{"name":"memory"}]' } });
    fireEvent.click(screen.getByRole("button", { name: "Review manifest changes" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("pinned version");
    expect(screen.queryByRole("button", { name: "Save reviewed manifest" })).toBeNull();
    expect(vi.mocked(onboardingRequest).mock.calls.every((call) => call[1] === undefined)).toBe(true);
  });
});

describe("device administration", () => {
  beforeEach(() => { vi.mocked(onboardingRequest).mockResolvedValue({ devices: [device] }); });
  it("labels unseen health unknown and revokes only the selected device after review", async () => {
    vi.mocked(onboardingRequest).mockImplementation(async (_suffix, body) => body ? { ...device, revoked_at: "2026-10-05T01:00:00Z" } : { devices: [device] });
    render(<DevicesPanel readOnly={false} />);
    expect(await screen.findByText("unknown")).toBeInTheDocument();
    expect(screen.getByText("never observed")).toBeInTheDocument();
    expect(screen.queryByText("healthy")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Review revoke Ada's Mac" }));
    expect(vi.mocked(onboardingRequest).mock.calls.filter((call) => call[1] !== undefined)).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Confirm device revocation" }));
    await waitFor(() => expect(onboardingRequest).toHaveBeenCalledWith("/devices/device-id/revoke", {}));
  });
  it("disables device revocation on read-only services", async () => {
    render(<DevicesPanel readOnly />);
    expect(await screen.findByRole("button", { name: "Review revoke Ada's Mac" })).toBeDisabled();
  });
  it("reports a failed revocation and retains the reviewed target", async () => {
    vi.mocked(onboardingRequest).mockImplementation(async (_suffix, body) => {
      if (body) throw new Error("Current session denied");
      return { devices: [device] };
    });
    render(<DevicesPanel readOnly={false} />);
    fireEvent.click(await screen.findByRole("button", { name: "Review revoke Ada's Mac" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm device revocation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Current session denied");
    expect(screen.getByLabelText("Device revocation review")).toHaveTextContent("device-id");
  });
});

it("bounds provider operation inventory to 100 metadata records", async () => {
  vi.mocked(onboardingRequest).mockResolvedValue({ operations: Array.from({ length: 101 }, (_, index) => ({
    id: `operation-${index}`, operation: "invite", outcome: "uncertain", provider_user_id: null, invitation_id: null,
  })) });
  render(<ProviderOperationsPanel refreshVersion={0} operationId={null} />);
  await screen.findByText("operation-0");
  expect(screen.getAllByRole("button", { name: /^Inspect operation/ })).toHaveLength(100);
  expect(screen.queryByText("operation-100")).toBeNull();
  expect(vi.mocked(onboardingRequest).mock.calls.every((call) => call[1] === undefined)).toBe(true);
});
