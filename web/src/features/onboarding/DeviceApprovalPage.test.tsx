import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { approveDevice, deviceReview, getAuthMe } from "../../api";
import type { AuthMeResponse } from "../../types";
import { DeviceApprovalPage } from "./DeviceApprovalPage";
import type { DeviceReview } from "./types";

vi.mock("../../api", async (importOriginal) => ({
  authLoginUrl: (await importOriginal<typeof import("../../api")>()).authLoginUrl,
  approveDevice: vi.fn(), deviceReview: vi.fn(), getAuthMe: vi.fn(),
}));
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const human: AuthMeResponse = { authenticated: true, mode: "multi_user", read_only: false,
  principal: { id: "human-id", display_name: "Ada", kind: "human_oidc", groups: [], projects: [] } };
const review: DeviceReview = {
  request_id: "request-id", device_id: "device-id", user_code: "ABCD-1234", name: "Ada's Mac", platform: "macos",
  credential_sha256: "a".repeat(64), expires_at: "2099-01-01T00:00:00Z",
  projects: [{ allowed_roles: ["reader", "writer"], offline_available: false, manifest: {
    project_id: "project-id", project: "memory", area: "trust", owner_instance_id: "owner-id",
    repository_url: "https://github.com/example/memory", default_branch: "main", offline_allowed: false,
    tools: [], destinations: [], revision: 3, reviewed_by: "admin-id", reviewed_at: "2026-10-05T00:00:00Z",
  } }],
};
beforeEach(() => {
  vi.mocked(getAuthMe).mockReset().mockResolvedValue(human);
  vi.mocked(deviceReview).mockReset().mockResolvedValue(review);
  vi.mocked(approveDevice).mockReset().mockResolvedValue({ status: "approved", device_id: review.device_id,
    principal_id: "machine-principal-id", projects: [{ manifest: review.projects[0].manifest, role: "reader" }] });
});

async function inspect() {
  render(<DeviceApprovalPage initialCode="ABCD-1234" />);
  await screen.findByText("Approving as Ada.");
  fireEvent.click(screen.getByRole("button", { name: "Review device request" }));
  await screen.findByRole("heading", { name: "Device identity" });
}
async function confirmSelection() {
  fireEvent.click(screen.getByLabelText("Select trust project project-id"));
  fireEvent.click(screen.getByRole("button", { name: "Review selected access" }));
  fireEvent.click(screen.getByLabelText("I verified this device fingerprint and the exact project identities and roles."));
}

describe("human device approval", () => {
  it("rejects local-owner and service credentials before making review requests", async () => {
    for (const identity of [{ ...human, mode: "single_user" as const, principal: { ...human.principal, kind: "internal" as const } },
      { ...human, principal: { ...human.principal, kind: "service_token" as const } }]) {
      vi.mocked(getAuthMe).mockResolvedValue(identity);
      render(<DeviceApprovalPage initialCode="ABCD-1234" />);
      expect(await screen.findByRole("link", { name: "Sign in to review this device" })).toHaveAttribute("href", "/v1/auth/login?return_to=%2F%3Fdevice_code%3DABCD-1234");
      expect(screen.getByRole("button", { name: "Review device request" })).toBeDisabled();
      cleanup();
    }
    expect(deviceReview).not.toHaveBeenCalled(); expect(approveDevice).not.toHaveBeenCalled();
  });

  it("shows exact identity and Trust policy, freezes selection and submits only confirmed revision-scoped access", async () => {
    const storage = vi.spyOn(Storage.prototype, "setItem");
    await inspect();
    expect(deviceReview).toHaveBeenCalledWith("ABCD-1234");
    expect(screen.getByText(review.credential_sha256)).toBeInTheDocument();
    expect(screen.getByText("owner-id")).toBeInTheDocument();
    expect(screen.getByText("Trust — remote-only; no offline copy")).toBeInTheDocument();
    expect(screen.queryByLabelText(/offline/i)).toBeNull();
    fireEvent.click(screen.getByLabelText("Select trust project project-id"));
    expect(within(screen.getByLabelText("Role for project project-id")).getAllByRole("option").map((option) => option.textContent)).toEqual(["reader", "writer"]);
    fireEvent.click(screen.getByRole("button", { name: "Review selected access" }));
    expect(screen.getByLabelText("Exact device approval")).toHaveTextContent("trust · project project-id · owner owner-id · revision 3 · reader · remote-only");
    expect(screen.getByLabelText("Select trust project project-id")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Approve selected device access" })).toBeDisabled();
    expect(approveDevice).not.toHaveBeenCalled();
    fireEvent.click(screen.getByLabelText("I verified this device fingerprint and the exact project identities and roles."));
    fireEvent.click(screen.getByRole("button", { name: "Approve selected device access" }));
    await waitFor(() => expect(approveDevice).toHaveBeenCalledWith({ user_code: "ABCD-1234", projects: [{ project_id: "project-id", role: "reader", revision: 3 }], confirmed: true }));
    expect(await screen.findByRole("heading", { name: "Device approval recorded" })).toBeInTheDocument();
    expect(storage).not.toHaveBeenCalled();
  });

  it("does not offer roles absent from current effective permissions", async () => {
    vi.mocked(deviceReview).mockResolvedValue({ ...review, projects: [{ ...review.projects[0], allowed_roles: ["reader"] }] });
    await inspect();
    fireEvent.click(screen.getByLabelText("Select trust project project-id"));
    expect(within(screen.getByLabelText("Role for project project-id")).getAllByRole("option").map((option) => option.textContent)).toEqual(["reader"]);
  });

  it("rejects unexpected privileged roles in a review response", async () => {
    vi.mocked(deviceReview).mockResolvedValue({ ...review, projects: [{ ...review.projects[0], allowed_roles: ["admin" as never] }] });
    render(<DeviceApprovalPage initialCode="ABCD-1234" />);
    await screen.findByText("Approving as Ada.");
    fireEvent.click(screen.getByRole("button", { name: "Review device request" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("unsupported project role");
    expect(screen.queryByRole("heading", { name: "Device identity" })).toBeNull();
    expect(approveDevice).not.toHaveBeenCalled();
  });

  it("blocks approval of an expired request", async () => {
    vi.mocked(deviceReview).mockResolvedValue({ ...review, expires_at: "2020-01-01T00:00:00Z" });
    await inspect();
    expect(await screen.findByRole("alert")).toHaveTextContent("expired");
    expect(screen.getByLabelText("Select trust project project-id")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Review selected access" })).toBeDisabled();
    expect(approveDevice).not.toHaveBeenCalled();
  });

  it("surfaces stale membership/revision errors and requires a new confirmation", async () => {
    vi.mocked(approveDevice).mockRejectedValue(new Error("Device request, project membership or reviewed manifest revision changed. Review the current request again."));
    await inspect(); await confirmSelection();
    fireEvent.click(screen.getByRole("button", { name: "Approve selected device access" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("manifest revision changed");
    expect(screen.queryByRole("heading", { name: "Device approval recorded" })).toBeNull();
    expect(screen.getByLabelText("I verified this device fingerprint and the exact project identities and roles.")).not.toBeChecked();
    expect(approveDevice).toHaveBeenCalledOnce();
  });

  it("rechecks human identity immediately before approval", async () => {
    await inspect(); await confirmSelection();
    vi.mocked(getAuthMe).mockResolvedValue({ ...human, mode: "single_user", principal: { ...human.principal, kind: "internal" } });
    fireEvent.click(screen.getByRole("button", { name: "Approve selected device access" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("human browser session is required");
    expect(approveDevice).not.toHaveBeenCalled();
  });

  it("requires a new review when another valid human replaces the reviewing identity", async () => {
    await inspect(); await confirmSelection();
    vi.mocked(getAuthMe).mockResolvedValue({ ...human, principal: { ...human.principal, id: "another-human", display_name: "Grace" } });
    fireEvent.click(screen.getByRole("button", { name: "Approve selected device access" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Human identity changed after review");
    expect(approveDevice).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("Exact device approval")).toBeNull();
    expect(screen.queryByRole("heading", { name: "Device identity" })).toBeNull();
    expect(screen.getByText("Approving as Grace.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Review device request" })).toBeEnabled();
  });

  it("rejects a receipt bound to another owner instance", async () => {
    vi.mocked(approveDevice).mockResolvedValue({ status: "approved", device_id: review.device_id, principal_id: "machine-id",
      projects: [{ role: "reader", manifest: { ...review.projects[0].manifest, owner_instance_id: "different-owner" } }] });
    await inspect(); await confirmSelection();
    fireEvent.click(screen.getByRole("button", { name: "Approve selected device access" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("receipt mismatch");
    expect(screen.queryByRole("heading", { name: "Device approval recorded" })).toBeNull();
  });

  it("blocks read-only service approvals and invalid code forwarding", async () => {
    vi.mocked(getAuthMe).mockResolvedValue({ ...human, read_only: true });
    render(<DeviceApprovalPage initialCode="ABCD-1234" />);
    expect(await screen.findByText(/This service is read-only/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Review device request" })).toBeDisabled();
    cleanup(); vi.mocked(getAuthMe).mockRejectedValue(new Error("401"));
    render(<DeviceApprovalPage initialCode="invalid-bearer-or-poll-secret" />);
    expect(await screen.findByRole("link", { name: "Sign in to review this device" })).toHaveAttribute("href", "/v1/auth/login?return_to=%2F%3Fdevice_code%3D");
    expect(screen.getByLabelText("User code")).toHaveValue("");
  });
});
