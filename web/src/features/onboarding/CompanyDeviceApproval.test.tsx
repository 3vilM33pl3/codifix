import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { approveCompanyDevice, approveDevice, companyDeviceReview, deviceReview, getAuthMe } from "../../api";
import type { AuthMeResponse } from "../../types";
import { DeviceApprovalPage } from "./DeviceApprovalPage";
import type { CompanyDeviceReview } from "./types";
vi.mock("../../api", async original => ({ authLoginUrl: (await original<typeof import("../../api")>()).authLoginUrl,
  approveCompanyDevice: vi.fn(), companyDeviceReview: vi.fn(), approveDevice: vi.fn(), deviceReview: vi.fn(), getAuthMe: vi.fn() }));
const id = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;
const human: AuthMeResponse = { authenticated: true, mode: "multi_user", read_only: false, principal: { id: id(1), display_name: "Ada", kind: "human_oidc", groups: [], projects: [] } };
const manifest = { project_id: id(2), project: "same-name", area: "workshop" as const, owner_instance_id: id(3), repository_url: "https://repo.example/project", default_branch: "reviewed-branch", offline_allowed: true,
  tools: [{ name: "Reviewed skill", version: "1.0", url: "https://tools.example/skill.zip", sha256: "c".repeat(64) }], destinations: [{ name: "Reviewed Sol", provider_id: id(4), destination_id: id(5) }], revision: 3, reviewed_by: id(6), reviewed_at: "2026-10-05T00:00:00Z" };
const first = { project: { manifest, allowed_roles: ["reader" as const], offline_available: true }, manifest_sha256: "a".repeat(64) };
const second = { project: { ...first.project, manifest: { ...manifest, owner_instance_id: id(7) } }, manifest_sha256: "b".repeat(64) };
const review: CompanyDeviceReview = { company_id: id(8), request_id: id(9), device_id: id(10), user_code: "ABCD-1234", name: "Reviewed Mac", platform: "macos", credential_sha256: "d".repeat(64), expires_at: "2099-01-01T00:00:00Z", observed_at: "2026-10-05T00:00:00Z", projects: [first, second] };
const check = (owner = id(3)) => `Select workshop project ${id(2)} owner ${owner}`;
const consent = "I verified the device fingerprint, exact owner/project revisions and digests, and permit retained offline copies.";
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
beforeEach(() => {
  vi.mocked(getAuthMe).mockReset().mockResolvedValue(human); vi.mocked(companyDeviceReview).mockReset().mockResolvedValue(review);
  vi.mocked(approveCompanyDevice).mockReset().mockResolvedValue({ status: "approved", device_id: review.device_id, principal_id: id(11), projects: [{ manifest, role: "reader" }] });
  vi.mocked(deviceReview).mockReset(); vi.mocked(approveDevice).mockReset();
});
async function inspect() {
  render(<DeviceApprovalPage initialCode="ABCD-1234" initialMode="company" />);
  await screen.findByText("Approving as Ada."); fireEvent.click(screen.getByText("Review company device request"));
  await screen.findByRole("heading", { name: "Device identity" });
}
function freeze(owner = id(3)) {
  fireEvent.click(screen.getByLabelText(check(owner))); fireEvent.click(screen.getByText("Review selected company access"));
}
async function submit() {
  fireEvent.click(screen.getByLabelText(consent));
  fireEvent.click(screen.getByRole("button", { name: /Approve selected company access|Retry exact company approval/ }));
}
it("distinguishes same project name and UUID across owners with composite selection and exact offline confirmation", async () => {
  const storage = vi.spyOn(Storage.prototype, "setItem");
  await inspect(); expect(companyDeviceReview).toHaveBeenCalledWith("ABCD-1234", expect.any(AbortSignal));
  expect(screen.getAllByText("same-name")).toHaveLength(2);
  expect(screen.getByLabelText(check(id(3)))).not.toBeChecked(); expect(screen.getByLabelText(check(id(7)))).not.toBeChecked();
  expect(screen.getAllByText(/https:\/\/tools.example\/skill.zip/)[0]).toHaveTextContent("c".repeat(64));
  expect(screen.getAllByText(/Reviewed Sol/)[0]).toHaveTextContent(id(4));
  freeze(id(7));
  const confirmation = screen.getByLabelText("Exact company device approval");
  expect(confirmation).toHaveTextContent(id(7)); expect(confirmation).toHaveTextContent('"manifest_sha256": "' + "b".repeat(64));
  expect(screen.getByRole("button", { name: "Approve selected company access" })).toBeDisabled();
  expect(screen.getByLabelText("Enrollment scope")).toBeDisabled();
  vi.mocked(approveCompanyDevice).mockResolvedValue({ status: "approved", device_id: review.device_id, principal_id: id(11), projects: [{ manifest: second.project.manifest, role: "reader" }] });
  await submit(); await screen.findByRole("heading", { name: "Company device approval recorded" });
  expect(approveCompanyDevice).toHaveBeenCalledWith({ user_code: review.user_code, confirmed: true, projects: [{ area: "workshop", owner_instance_id: id(7), project_id: id(2), manifest_revision: 3, manifest_sha256: "b".repeat(64), offline_confirmed: true }] }, expect.any(AbortSignal));
  expect(deviceReview).not.toHaveBeenCalled(); expect(approveDevice).not.toHaveBeenCalled(); expect(storage).not.toHaveBeenCalled();
});
it("never offers absent projects and cannot select unavailable offline or Trust entries", async () => {
  const trust = { ...first, project: { ...first.project, manifest: { ...manifest, area: "trust" as const, owner_instance_id: id(12) } } };
  vi.mocked(companyDeviceReview).mockResolvedValue({ ...review, projects: [{ ...first, project: { ...first.project, offline_available: false } }, trust] });
  await inspect(); expect(screen.getByLabelText(check())).toBeDisabled();
  expect(screen.getByLabelText(`Select trust project ${id(2)} owner ${id(12)}`)).toBeDisabled();
  expect(screen.getByText("Trust — remote-only; no offline copy")).toBeInTheDocument();
  expect(screen.queryByLabelText(check(id(7)))).toBeNull(); expect(screen.queryByRole("combobox", { name: /role/i })).toBeNull();
});
it("freezes explicit curated upload consent only for an available selected project", async () => {
  vi.mocked(companyDeviceReview).mockResolvedValue({ ...review, projects: [{ ...first, curated_contributions_available: true }] });
  await inspect();
  const upload = screen.getByLabelText(`Allow curated uploads for ${manifest.project_id}`);
  expect(upload).toBeDisabled();
  fireEvent.click(screen.getByLabelText(check()));
  fireEvent.click(upload);
  fireEvent.click(screen.getByText("Review selected company access"));
  expect(screen.getByLabelText("Exact company device approval")).toHaveTextContent('"contribution_confirmed": true');
  await submit();
  await waitFor(() => expect(approveCompanyDevice).toHaveBeenCalledOnce());
  expect(vi.mocked(approveCompanyDevice).mock.calls[0][0].projects[0].contribution_confirmed).toBe(true);
});
it.each(["pending", "uncertain"])("retains the identical frozen selection after %s without refresh or fallback", async outcome => {
  if (outcome === "pending") vi.mocked(approveCompanyDevice).mockResolvedValueOnce({ status: "pending" });
  else vi.mocked(approveCompanyDevice).mockRejectedValueOnce(new Error("Company device outcome unconfirmed; retry exact bytes"));
  await inspect(); freeze(); await submit();
  await waitFor(() => expect(screen.getByRole("button", { name: "Retry exact company approval" })).toBeDisabled());
  expect(screen.getByText("Change company selection")).toBeDisabled(); expect(screen.getByText("Review company device request")).toBeDisabled();
  expect(screen.getByLabelText("Enrollment scope")).toBeDisabled(); expect(companyDeviceReview).toHaveBeenCalledOnce();
  const original = vi.mocked(approveCompanyDevice).mock.calls[0][0];
  await submit(); await screen.findByText("Company device approval recorded");
  expect(vi.mocked(approveCompanyDevice).mock.calls[1][0]).toEqual(original);
  expect(companyDeviceReview).toHaveBeenCalledOnce(); expect(approveDevice).not.toHaveBeenCalled();
});
it("rejects changed approval revision/manifest instead of presenting success", async () => {
  vi.mocked(approveCompanyDevice).mockResolvedValue({ status: "approved", device_id: review.device_id, principal_id: id(11), projects: [{ manifest: { ...manifest, revision: 4 }, role: "reader" }] });
  await inspect(); freeze(); await submit();
  expect(await screen.findByRole("alert")).toHaveTextContent("scope/revision/manifest mismatch");
  expect(screen.queryByText("Company device approval recorded")).toBeNull(); expect(screen.getByLabelText("Exact company device approval")).toHaveTextContent('"manifest_revision": 3');
});
it("explicit re-review invalidates previous revisions and digest selections before confirmation", async () => {
  await inspect(); fireEvent.click(screen.getByLabelText(check()));
  vi.mocked(companyDeviceReview).mockResolvedValue({ ...review, projects: [{ ...first, manifest_sha256: "e".repeat(64), project: { ...first.project, manifest: { ...manifest, revision: 4 } } }] });
  fireEvent.click(screen.getByText("Review company device request"));
  await screen.findByText(/revision 4/); expect(screen.getByLabelText(check())).not.toBeChecked();
  freeze(); expect(screen.getByLabelText("Exact company device approval")).toHaveTextContent("e".repeat(64));
  expect(screen.getByLabelText("Exact company device approval")).not.toHaveTextContent('"manifest_revision": 3');
});
it("cancel aborts and discards late review responses", async () => {
  let resolve!: (value: CompanyDeviceReview) => void;
  vi.mocked(companyDeviceReview).mockReturnValue(new Promise(r => { resolve = r; }));
  render(<DeviceApprovalPage initialCode="ABCD-1234" initialMode="company" />); await screen.findByText("Approving as Ada.");
  fireEvent.click(screen.getByText("Review company device request")); await waitFor(() => expect(companyDeviceReview).toHaveBeenCalledOnce());
  fireEvent.click(screen.getByText("Cancel review")); expect(vi.mocked(companyDeviceReview).mock.calls[0][1]?.aborted).toBe(true);
  resolve(review); await waitFor(() => expect(screen.queryByRole("heading", { name: "Device identity" })).toBeNull());
});
it("requires the reviewing human again and blocks read-only/local-owner approval", async () => {
  await inspect(); freeze(); vi.mocked(getAuthMe).mockResolvedValue({ ...human, principal: { ...human.principal, id: id(20) } });
  await submit(); expect(await screen.findByRole("alert")).toHaveTextContent("Human identity changed"); expect(approveCompanyDevice).not.toHaveBeenCalled();
  expect(screen.queryByLabelText("Exact company device approval")).toBeNull();
  cleanup(); vi.mocked(getAuthMe).mockResolvedValue({ ...human, read_only: true });
  render(<DeviceApprovalPage initialCode="ABCD-1234" initialMode="company" />); await screen.findByText(/service is read-only/);
  expect(screen.getByText("Review company device request")).toBeDisabled();
  cleanup(); vi.mocked(getAuthMe).mockResolvedValue({ ...human, mode: "single_user", principal: { ...human.principal, kind: "internal" } });
  render(<DeviceApprovalPage initialCode="ABCD-1234" initialMode="company" />);
  expect(await screen.findByRole("link", { name: "Sign in to review this device" })).toHaveAttribute("href", "/v1/auth/login?return_to=%2F%3Fdevice_code%3DABCD-1234%26company_device%3D1");
});
it("errors and revoked grants never downgrade to legacy, and writer roles are rejected", async () => {
  vi.mocked(companyDeviceReview).mockRejectedValue(new Error("Company owner revoked or unavailable"));
  render(<DeviceApprovalPage initialCode="ABCD-1234" initialMode="company" />); await screen.findByText("Approving as Ada.");
  fireEvent.click(screen.getByText("Review company device request")); expect(await screen.findByRole("alert")).toHaveTextContent("revoked");
  expect(deviceReview).not.toHaveBeenCalled(); expect(approveDevice).not.toHaveBeenCalled();
  cleanup(); vi.mocked(companyDeviceReview).mockResolvedValue({ ...review, projects: [{ ...first, project: { ...first.project, allowed_roles: ["writer"] } }] });
  render(<DeviceApprovalPage initialCode="ABCD-1234" initialMode="company" />); await screen.findByText("Approving as Ada.");
  fireEvent.click(screen.getByText("Review company device request")); expect(await screen.findByRole("alert")).toHaveTextContent("reader role or manifest digest invalid");
  expect(screen.queryByRole("heading", { name: "Device identity" })).toBeNull();
});
it("expired challenges and malformed reviewed digests cannot produce confirmed selections", async () => {
  vi.mocked(companyDeviceReview).mockResolvedValue({ ...review, expires_at: "2020-01-01T00:00:00Z" });
  await inspect(); expect(screen.getByRole("alert")).toHaveTextContent("expired"); expect(screen.getByLabelText(check())).toBeDisabled();
  cleanup(); vi.mocked(companyDeviceReview).mockResolvedValue({ ...review, projects: [{ ...first, manifest_sha256: "not-a-reviewed-hash" }] });
  render(<DeviceApprovalPage initialCode="ABCD-1234" initialMode="company" />); await screen.findByText("Approving as Ada.");
  fireEvent.click(screen.getByText("Review company device request")); expect(await screen.findByRole("alert")).toHaveTextContent("manifest digest invalid");
  expect(approveCompanyDevice).not.toHaveBeenCalled();
});
