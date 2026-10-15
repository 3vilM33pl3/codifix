import { StrictMode } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { onboardingRequest } from "../../api";
import { CompanyDelegatesPanel } from "./CompanyDelegatesPanel";
import type { CompanyDelegateResponse, CompanyDeliveryDelegatePlan, CompanyDeliveryDelegateRequest, CompanyManagementDelegatePlan, CompanyManagementDelegateRequest } from "./types";

vi.mock("../../api", () => ({ onboardingRequest: vi.fn() }));
const id = (value: number) => `00000000-0000-4000-8000-${String(value).padStart(12, "0")}`;
const identity = { company_id: id(1), owner_instance_id: id(2), area: "workshop" as const,
  issuer: "https://id.example.test", principal_id: id(3), token_id: id(4) };
const management: CompanyManagementDelegatePlan = { ...identity, delegation_policy_revision: 3,
  projects: [{ project_id: id(5), role_ceiling: "operator" }], status: "unprovisioned", expires_at: null };
const delivery: CompanyDeliveryDelegatePlan = { ...identity, principal_id: id(6), token_id: id(7), delivery_policy_revision: 9,
  projects: [id(5)], permissions: ["project_scoped_delivery"], status: "unprovisioned", expires_at: null };
type Request = CompanyManagementDelegateRequest | CompanyDeliveryDelegateRequest;
const secret = "memory-secret-must-never-render";
function response(request: Request, token: string | null = secret): CompanyDelegateResponse {
  const { name: _name, expires_in_days: _days, confirmed: _confirmed, ...rest } = request;
  const common = { ...rest, expires_at: "2027-01-01T00:00:00Z", replayed: !token, credential_issued: !!token, token };
  return "delegation_policy_revision" in request
    ? { ...common, delegation_policy_revision: request.delegation_policy_revision, projects: request.projects.map((project) => ({ ...project, permissions: ["memory_read", "auth_manage"] })) }
    : { ...common, delivery_policy_revision: request.delivery_policy_revision, projects: request.projects, permissions: ["project_scoped_delivery"] };
}
let post: ReturnType<typeof vi.fn<(request: Request) => Promise<CompanyDelegateResponse>>>;
beforeEach(() => {
  vi.mocked(onboardingRequest).mockReset();
  post = vi.fn(async (request: Request) => response(request));
  vi.mocked(onboardingRequest).mockImplementation(async (suffix, body) => body ? post(body as Request)
    : { delegates: suffix === "/company/delegates" ? [management] : [delivery] });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
const consent = "I reviewed this exact owner, company, identity, policy, projects and credential lifetime.";
async function prepare(kind = "management") {
  await waitFor(() => expect(screen.getByRole("button", { name: "Refresh configured delegates" })).toBeEnabled());
  fireEvent.change(screen.getByLabelText("Delegate credential name"), { target: { value: "Company owner delegate" } });
  fireEvent.click(screen.getByRole("button", { name: new RegExp(`Review ${kind} delegate`) }));
}
function submit() {
  fireEvent.click(screen.getByLabelText(consent));
  fireEvent.click(screen.getByRole("button", { name: "Provision reviewed delegate" }));
}

it("lists exact owner plans in strict mode and requires explicit review and consent before provisioning", async () => {
  render(<StrictMode><CompanyDelegatesPanel /></StrictMode>);
  await prepare();
  expect(screen.getByRole("button", { name: "Provision reviewed delegate" })).toBeDisabled();
  expect(screen.getByLabelText("Delegate credential name")).toBeDisabled();
  expect(screen.getByLabelText("Delegate lifetime in days")).toHaveValue(90);
  expect(post).not.toHaveBeenCalled();
  submit();
  await screen.findByRole("button", { name: "Download private delegate receipt" });
  expect(post).toHaveBeenCalledOnce();
  expect(post.mock.calls[0][0]).toEqual({ ...identity, delegation_policy_revision: 3, projects: management.projects,
    name: "Company owner delegate", expires_in_days: 90, confirmed: true, request_id: expect.any(String) });
  expect(document.body.innerHTML).not.toContain(secret);
  expect(screen.getByRole("button", { name: "Retry exact delegate request" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Refresh configured delegates" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Close delegate review" })).toBeDisabled();
});

it("maps delivery UUID projects and policy without management roles or read-only plan metadata", async () => {
  render(<CompanyDelegatesPanel />);
  await prepare("delivery");
  submit();
  await screen.findByRole("button", { name: "Download private delegate receipt" });
  expect(post.mock.calls[0][0]).toEqual({ ...identity, principal_id: id(6), token_id: id(7),
    delivery_policy_revision: 9, projects: [id(5)], name: "Company owner delegate", expires_in_days: 90, confirmed: true, request_id: expect.any(String) });
  expect(onboardingRequest).toHaveBeenCalledWith("/company/delivery-delegates", post.mock.calls[0][0]);
  expect(document.body.innerHTML).not.toContain(secret);
});

it("downloads a private JSON Blob with unique identity filename and clears only after saved acknowledgement", async () => {
  const create = vi.fn((_blob: Blob) => "blob:private-receipt");
  vi.stubGlobal("URL", { createObjectURL: create, revokeObjectURL: vi.fn() });
  const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  render(<CompanyDelegatesPanel />);
  await prepare(); submit();
  await screen.findByRole("button", { name: "Download private delegate receipt" });
  expect(screen.getByRole("button", { name: "I saved the private receipt" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Download private delegate receipt" }));
  expect(create.mock.calls[0][0]).toBeInstanceOf(Blob);
  expect(create.mock.calls[0][0].type).toBe("application/json");
  const blob = create.mock.calls[0][0];
  const contents = await new Promise<string>((resolve) => {
    const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.readAsText(blob);
  });
  expect(JSON.parse(contents)).toMatchObject({ token: secret, request_id: post.mock.calls[0][0].request_id });
  const anchor = click.mock.instances[0] as HTMLAnchorElement;
  expect(anchor.download).toBe(`company-delegate-${id(2)}-management-${post.mock.calls[0][0].request_id}.json`);
  expect(document.body.innerHTML).not.toContain(secret);
  expect(localStorage.length).toBe(0); expect(sessionStorage.length).toBe(0);
  expect(screen.getByRole("button", { name: "Refresh configured delegates" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "I saved the private receipt" }));
  expect(screen.queryByRole("button", { name: "Download private delegate receipt" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Close delegate review" }));
  expect(screen.getByRole("button", { name: "Refresh configured delegates" })).toBeEnabled();
  expect(screen.getByRole("button", { name: /Review management delegate/ })).toBeDisabled();
});

it("retains the exact request for an uncertain retry and explains replay without a recoverable token", async () => {
  post.mockRejectedValueOnce(new Error(`lost response ${secret}`)).mockImplementation(async (request) => response(request, null));
  render(<CompanyDelegatesPanel />);
  await prepare(); submit();
  expect(await screen.findByRole("alert")).toHaveTextContent(post.mock.calls[0][0].request_id);
  expect(screen.getByRole("button", { name: "Close delegate review" })).toBeDisabled();
  expect(screen.getByLabelText(consent)).toBeDisabled();
  expect(document.body.innerHTML).not.toContain(secret);
  fireEvent.click(screen.getByRole("button", { name: "Retry exact delegate request" }));
  expect(await screen.findByText(/one-time tokens cannot be recovered/)).toBeInTheDocument();
  expect(post).toHaveBeenCalledTimes(2);
  expect(post.mock.calls[1][0]).toEqual(post.mock.calls[0][0]);
  expect(screen.getByRole("button", { name: "Retry exact delegate request" })).toBeDisabled();
  expect(screen.queryByRole("button", { name: "Download private delegate receipt" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Close delegate review" }));
  expect(screen.getByRole("button", { name: /Review management delegate/ })).toBeDisabled();
});

it("suppresses duplicate submission while a request is pending", async () => {
  let finish!: (receipt: CompanyDelegateResponse) => void;
  post.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
  render(<CompanyDelegatesPanel />);
  await prepare(); submit();
  fireEvent.click(screen.getByRole("button", { name: "Retry exact delegate request" }));
  expect(post).toHaveBeenCalledOnce();
  expect(screen.getByRole("button", { name: "Refresh configured delegates" })).toBeDisabled();
  finish(response(post.mock.calls[0][0]));
  await screen.findByRole("button", { name: "Download private delegate receipt" });
});

it("allows read-only plan inspection but denies provisioning including a pending review", async () => {
  const { rerender } = render(<CompanyDelegatesPanel />);
  await prepare();
  fireEvent.click(screen.getByLabelText(consent));
  rerender(<CompanyDelegatesPanel readOnly />);
  expect(screen.getByRole("button", { name: "Provision reviewed delegate" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Provision reviewed delegate" }));
  expect(post).not.toHaveBeenCalled();
});

it("bounds lifetime and does not reprovision active, inactive or conflicting configured credentials", async () => {
  vi.mocked(onboardingRequest).mockResolvedValue({ delegates: ["active", "inactive", "conflict"].map((status, index) => ({ ...management, status, token_id: id(10 + index) })) });
  render(<CompanyDelegatesPanel />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Refresh configured delegates" })).toBeEnabled());
  fireEvent.change(screen.getByLabelText("Delegate credential name"), { target: { value: "Delegate" } });
  for (const button of screen.getAllByRole("button", { name: /Review .* delegate/ })) expect(button).toBeDisabled();
  expect(post).not.toHaveBeenCalled();
});

it("rejects out-of-bounds lifetime and preserves a mismatched issued receipt for reconciliation without exposing the secret", async () => {
  post.mockImplementation(async (request) => ({ ...response(request), company_id: id(90) }));
  render(<CompanyDelegatesPanel />);
  await prepare();
  fireEvent.click(screen.getByRole("button", { name: "Close delegate review" }));
  fireEvent.change(screen.getByLabelText("Delegate lifetime in days"), { target: { value: "91" } });
  expect(screen.getByRole("button", { name: /Review management delegate/ })).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Delegate lifetime in days"), { target: { value: "0" } });
  expect(screen.getByRole("button", { name: /Review management delegate/ })).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Delegate lifetime in days"), { target: { value: "1" } });
  fireEvent.click(screen.getByRole("button", { name: /Review management delegate/ })); submit();
  expect(await screen.findByRole("alert")).toHaveTextContent("Provisioning outcome unconfirmed");
  expect(screen.getByRole("button", { name: "Download private delegate receipt" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Retry exact delegate request" })).toBeDisabled();
  expect(document.body.innerHTML).not.toContain(secret);
});
