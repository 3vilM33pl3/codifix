import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { onboardingRequest } from "../../api";
import { CompanyPanel } from "./CompanyPanel";
import type { CompanyConfiguration, CompanyGrantRequest, CompanyOperation } from "./types";
vi.mock("../../api", () => ({ onboardingRequest: vi.fn() }));
afterEach(cleanup);
beforeEach(() => { vi.mocked(onboardingRequest).mockReset(); });
const project = "11111111-1111-4111-8111-111111111111";
const config: CompanyConfiguration = { company_id: "22222222-2222-4222-8222-222222222222", writes_enabled: true,
  owners: [
    { owner_instance_id: "33333333-3333-4333-8333-333333333333", area: "workshop", issuer: "https://id.example.test/shared", delegation_policy_revision: 3, grant_management: true },
    { owner_instance_id: "44444444-4444-4444-8444-444444444444", area: "trust", issuer: "https://id.example.test/shared", delegation_policy_revision: 1, grant_management: false },
  ] };
function operation(request: CompanyGrantRequest, current = true): CompanyOperation {
  return { request, request_sha256: "a".repeat(64), outcome: "confirmed",
    receipt: { request, request_sha256: "a".repeat(64), principal_id: "55555555-5555-4555-8555-555555555555", grant_revision: 1, direct_grant_active: request.operation === "grant", committed_at: "2026-10-05T10:00:00Z" },
    current_access: current ? { observed_at: "2026-10-05T10:01:00Z", principal_id: "55555555-5555-4555-8555-555555555555", disabled: false, project: "memory", direct_role: null, grant_company_id: request.company_id, owner_grant_revision: 1, group_roles: ["reader"], permissions: ["memory_read"] } : null };
}
function api(submit: (request: CompanyGrantRequest) => Promise<CompanyOperation>, history: CompanyOperation[] = []) {
  vi.mocked(onboardingRequest).mockImplementation(async (suffix, body) => {
    if (suffix === "/company/configuration") return config;
    if (suffix === "/company/operations") return body ? submit(body as CompanyGrantRequest) : { operations: history };
    if (suffix.endsWith("/reconcile")) return operation(history[0].request);
    throw new Error("Unexpected endpoint");
  });
}
async function prepare() {
  await screen.findByRole("option", { name: /workshop/ });
  fireEvent.change(screen.getByLabelText("Exact project ID"), { target: { value: project } });
  fireEvent.change(screen.getByLabelText("Verified user subject"), { target: { value: "verified-user-42" } });
  fireEvent.click(screen.getByRole("button", { name: "Review company grant" }));
}
function confirm() { fireEvent.click(screen.getByLabelText("I reviewed this exact owner, project, stable subject, role and revision.")); }
it("reviews stable identities and reports surviving group access after direct revoke", async () => {
  const submit = vi.fn(async (request: CompanyGrantRequest) => operation(request)); api(submit);
  render(<CompanyPanel />);
  await screen.findByRole("option", { name: /workshop/ });
  fireEvent.change(screen.getByLabelText("Direct grant action"), { target: { value: "revoke" } });
  await prepare();
  expect(submit).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Submit reviewed company grant" })).toBeDisabled();
  expect(screen.getByLabelText("Exact project ID")).toBeDisabled();
  confirm(); fireEvent.click(screen.getByRole("button", { name: "Submit reviewed company grant" }));
  await waitFor(() => expect(submit).toHaveBeenCalledOnce());
  const sent = submit.mock.calls[0][0];
  expect(sent).toMatchObject({ company_id: config.company_id, project_id: project, subject: "verified-user-42", issuer: config.owners[0].issuer, owner_instance_id: config.owners[0].owner_instance_id, area: "workshop", delegation_policy_revision: 3, operation: "revoke", role: "reader" });
  expect(await screen.findByText("Direct role: none; group roles: reader.")).toBeInTheDocument();
  expect(screen.getByText(/Revoking a direct grant does not remove/)).toBeInTheDocument();
});
it("keeps the exact reviewed request after a lost reply rather than making a new operation", async () => {
  let attempts = 0;
  const submit = vi.fn(async (request: CompanyGrantRequest) => {
    if (++attempts === 1) throw new Error("Connection lost");
    return operation(request);
  }); api(submit);
  render(<CompanyPanel />); await prepare(); confirm();
  fireEvent.click(screen.getByRole("button", { name: "Submit reviewed company grant" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(submit.mock.calls[0][0].operation_id);
  expect(screen.getByRole("button", { name: "Close local review" })).toBeDisabled();
  expect(screen.getByLabelText("Verified user subject")).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Retry same company request" }));
  await waitFor(() => expect(submit).toHaveBeenCalledTimes(2));
  expect(submit.mock.calls[1][0]).toEqual(submit.mock.calls[0][0]);
});
it("restores durable history without treating a receipt as current access or resubmitting", async () => {
  const request: CompanyGrantRequest = { company_id: config.company_id!, operation_id: project, owner_instance_id: config.owners[0].owner_instance_id, area: "workshop", project_id: project, issuer: config.owners[0].issuer, subject: "verified-user-42", operation: "grant", role: "reader", expected_grant_revision: 0, delegation_policy_revision: 3 };
  const submit = vi.fn(async () => operation(request)); api(submit, [operation(request, false)]);
  render(<CompanyPanel />);
  expect(await screen.findByText(/Historical grant revision: 1/)).toBeInTheDocument();
  expect(screen.queryByText(/Owner rights observed/)).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: `Reconcile ${project}` }));
  expect(await screen.findByText(/Owner rights observed at/)).toBeInTheDocument();
  expect(submit).not.toHaveBeenCalled();
  expect(onboardingRequest).toHaveBeenCalledWith(`/company/operations/${project}/reconcile`, {});
});
it("denies read-only changes and keeps Trust remote-only", async () => {
  const submit = vi.fn(async (request: CompanyGrantRequest) => operation(request)); api(submit);
  render(<CompanyPanel readOnly />);
  expect(await screen.findByRole("option", { name: /trust.*remote expertise only/ })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Review company grant" })).toBeDisabled();
  expect(screen.getByLabelText("Owning area")).toBeDisabled();
  expect(submit).not.toHaveBeenCalled();
});
it("rejects a mismatched receipt without presenting false effective permissions", async () => {
  api(async (request) => operation({ ...request, project_id: config.owners[0].owner_instance_id }));
  render(<CompanyPanel />); await prepare(); confirm();
  fireEvent.click(screen.getByRole("button", { name: "Submit reviewed company grant" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Company receipt mismatch");
  expect(screen.queryByText(/Owner rights observed/)).not.toBeInTheDocument();
});
