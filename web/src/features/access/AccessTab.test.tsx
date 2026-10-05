import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AccessTab } from "./AccessTab";
import {
  createAuthToken,
  listAuthMemberships,
  listAuthPrincipals,
  listAuthTokens,
  onboardingRequest,
} from "../../api";

vi.mock("../../api", () => ({
  createAuthToken: vi.fn(),
  grantAuthMembership: vi.fn(),
  listAuthMemberships: vi.fn(),
  listAuthPrincipals: vi.fn(),
  listAuthTokens: vi.fn(),
  revokeAuthMembership: vi.fn(),
  revokeAuthToken: vi.fn(),
  onboardingRequest: vi.fn(),
}));

const principal = {
  id: "00000000-0000-0000-0000-000000000001",
  kind: "human_oidc" as const,
  display_name: "Memory Admin",
  email: "admin@example.test",
  groups: ["memory-admins"],
  global_role: "admin" as const,
  projects: [],
};

describe("AccessTab", () => {
  afterEach(cleanup);
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listAuthPrincipals).mockResolvedValue([principal]);
    vi.mocked(listAuthTokens).mockResolvedValue([]);
    vi.mocked(listAuthMemberships).mockResolvedValue([]);
    vi.mocked(onboardingRequest).mockImplementation(async (suffix) => suffix === "/capabilities"
      ? { schema_version: 1, idp_management_enabled: false, invitation_enabled: false, idp_provider: null }
      : { users: [] });
  });

  it("loads access inventory and shows a newly issued token only in the result panel", async () => {
    vi.mocked(createAuthToken).mockResolvedValue({
      id: "00000000-0000-0000-0000-000000000002",
      principal_id: "00000000-0000-0000-0000-000000000003",
      name: "hermes",
      token_prefix: "mlt_example",
      created_at: "2026-08-18T12:00:00Z",
      token: "mlt_one_time_secret",
      projects: [{ project: "memory", role: "reader", source: "explicit" }],
    });

    render(<AccessTab project="memory" />);
    expect((await screen.findAllByText("Memory Admin")).length).toBeGreaterThan(0);

    const nameInput = screen.getByLabelText("Name");
    fireEvent.change(nameInput, { target: { value: "hermes" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => expect(createAuthToken).toHaveBeenCalledWith({
      name: "hermes",
      project: "memory",
      role: "reader",
    }));
    expect(await screen.findByText("mlt_one_time_secret")).toBeInTheDocument();
    expect(screen.getByText(/shown once/i)).toBeInTheDocument();
    vi.mocked(onboardingRequest).mockResolvedValue({ schema_version: 1, projects: [] });
    fireEvent.click(screen.getByRole("button", { name: "Projects" }));
    expect(screen.getByText("mlt_one_time_secret")).not.toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Users" }));
    expect(screen.getByText("mlt_one_time_secret")).toBeVisible();
  });

  it("disables legacy token and membership mutations on a read-only service", async () => {
    vi.mocked(listAuthTokens).mockResolvedValue([{
      id: "token-id", principal_id: "worker-id", name: "Worker", token_prefix: "mlt_worker",
      created_at: "2026-10-05T00:00:00Z", projects: [],
    }]);
    vi.mocked(listAuthMemberships).mockResolvedValue([{
      id: "membership-id", principal_id: principal.id, project: "memory", role: "reader",
      source: "explicit", created_at: "2026-10-05T00:00:00Z",
    }]);
    render(<AccessTab project="memory" readOnly />);
    await screen.findAllByText("Memory Admin");
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "worker" } });
    expect(screen.getByRole("button", { name: "Create" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Grant" })).toBeDisabled();
    for (const button of screen.getAllByRole("button", { name: "Revoke" })) expect(button).toBeDisabled();
  });

  it("retains legacy controls when onboarding inventory is unavailable", async () => {
    vi.mocked(onboardingRequest).mockRejectedValue(new Error("Administration unavailable"));
    render(<AccessTab project="memory" />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Administration unavailable");
    expect(screen.getByRole("heading", { name: "Create service token" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Grant project access" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Projects" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Devices" })).toBeInTheDocument();
  });
});
