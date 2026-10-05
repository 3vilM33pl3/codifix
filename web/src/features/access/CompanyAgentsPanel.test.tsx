import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { CompanyAgentsPanel } from "./CompanyAgentsPanel";
import { AccessTab } from "./AccessTab";
import { approveCompanyAgent, CompanyAgentRequestError, getAuthMe, listCompanyAgents, revokeCompanyAgent, onboardingRequest, listAuthPrincipals, listAuthMemberships, listAuthTokens } from "../../api";
import { aid, agentEnvelope, agentHuman, agentReceipt } from "./companyAgentFixtures.testdata";
vi.mock("../../api", async importOriginal => ({ ...(await importOriginal<typeof import("../../api")>()), getAuthMe: vi.fn(), listCompanyAgents: vi.fn(), approveCompanyAgent: vi.fn(), revokeCompanyAgent: vi.fn(), onboardingRequest: vi.fn(), listAuthPrincipals: vi.fn(), listAuthMemberships: vi.fn(), listAuthTokens: vi.fn() }));
beforeEach(() => { vi.resetAllMocks(); vi.mocked(getAuthMe).mockResolvedValue(agentHuman()); vi.mocked(listCompanyAgents).mockResolvedValue({ schema_version: 1, agents: [] }); });
afterEach(cleanup);
const consent = () => fireEvent.click(screen.getByLabelText(/I verified the agent fingerprint/));
async function review(value = agentEnvelope(), readonly = false) {
  render(<CompanyAgentsPanel readOnly={readonly} />); await screen.findByText(/Reviewing as Parent Reader/);
  fireEvent.change(screen.getByLabelText(/Public agent envelope \(JSON/), { target: { value: JSON.stringify(value) } });
  fireEvent.click(screen.getByRole("button", { name: "Review public agent request" })); await screen.findByLabelText("Exact company agent request"); return value;
}
it("requires explicit consent, discloses full owner identities and never autoapproves preloaded public links", async () => {
  const value = agentEnvelope(); vi.mocked(approveCompanyAgent).mockResolvedValue(agentReceipt(value));
  render(<CompanyAgentsPanel readOnly={false} initialRequest={JSON.stringify(value)} />);
  await screen.findByLabelText("Exact company agent request"); expect(approveCompanyAgent).not.toHaveBeenCalled();
  expect(screen.getByLabelText("Exact company agent request")).toHaveTextContent(aid(7)); expect(screen.getByLabelText("Exact company agent request")).toHaveTextContent(aid(9));
  expect(screen.getByRole("button", { name: "Approve separate agent identity" })).toBeDisabled(); consent(); fireEvent.click(screen.getByRole("button", { name: "Approve separate agent identity" }));
  await waitFor(() => expect(approveCompanyAgent).toHaveBeenCalledWith(aid(2), { ...value.request, confirmed: true }, expect.any(AbortSignal)));
  expect(await screen.findByText(/Agent approval recorded as metadata/)).toHaveTextContent("no model call or credential delivery");
});
it("freezes the exact body on lost confirmation and retries only after renewed consent", async () => {
  const value = await review(); vi.mocked(approveCompanyAgent).mockRejectedValueOnce(new CompanyAgentRequestError("Agent outcome unconfirmed. Retry unchanged IDs.", true)).mockResolvedValueOnce(agentReceipt(value));
  consent(); fireEvent.click(screen.getByRole("button", { name: "Approve separate agent identity" })); await screen.findByRole("alert");
  expect(screen.getByLabelText(/Public agent envelope \(JSON/)).toBeDisabled(); expect(screen.getByRole("button", { name: "Retry exact agent approval" })).toBeDisabled();
  consent(); fireEvent.click(screen.getByRole("button", { name: "Retry exact agent approval" })); await screen.findByText(/Agent approval recorded as metadata/);
  const calls = vi.mocked(approveCompanyAgent).mock.calls; expect(calls).toHaveLength(2); expect(calls[0][1]).toEqual(calls[1][1]);
});
it("restores historical receipt from refresh after a lost reply without reactivating or resubmitting", async () => {
  const value = await review(); vi.mocked(approveCompanyAgent).mockRejectedValue(new Error("PRIVATE_PROVIDER_ERROR"));
  consent(); fireEvent.click(screen.getByRole("button", { name: "Approve separate agent identity" })); expect(await screen.findByRole("alert")).not.toHaveTextContent("PRIVATE_PROVIDER_ERROR");
  vi.mocked(listCompanyAgents).mockResolvedValue({ schema_version: 1, agents: [agentReceipt(value)] }); fireEvent.click(screen.getByRole("button", { name: "Refresh own device agent receipts" }));
  await screen.findByText(/Matching historical approval receipt/); expect(approveCompanyAgent).toHaveBeenCalledOnce(); expect(screen.queryByRole("button", { name: "Retry exact agent approval" })).not.toBeInTheDocument();
});
it.each([403, 409])("keeps definitive %s failures frozen without a changed retry", async status => {
  await review(); vi.mocked(approveCompanyAgent).mockRejectedValue(new CompanyAgentRequestError(`Sanitized ${status} denial`, false)); consent(); fireEvent.click(screen.getByRole("button", { name: "Approve separate agent identity" })); await screen.findByRole("alert"); consent();
  expect(screen.getByRole("button", { name: "Retry exact agent approval" })).toBeDisabled(); expect(approveCompanyAgent).toHaveBeenCalledOnce();
});
it.each(["readonly", "changed-human", "local-owner"])("denies mutation on %s at the current session boundary", async state => {
  await review(undefined, state === "readonly");
  if (state === "readonly") { expect(screen.getByLabelText(/I verified/)).toBeDisabled(); }
  else { const changed = agentHuman(); changed.principal = { ...changed.principal, id: aid(25) }; if (state === "local-owner") changed.mode = "single_user"; vi.mocked(getAuthMe).mockResolvedValue(changed); consent(); fireEvent.click(screen.getByRole("button", { name: "Approve separate agent identity" })); await screen.findByRole("alert"); }
  expect(approveCompanyAgent).not.toHaveBeenCalled();
});
it("requires owning-parent inventory authorization and rejects mismatched receipt digest", async () => {
  const value = await review(); const receipt = agentReceipt(value); receipt.projects[0].selection = { ...receipt.projects[0].selection, manifest_sha256: "d".repeat(64) }; vi.mocked(approveCompanyAgent).mockResolvedValue(receipt);
  consent(); fireEvent.click(screen.getByRole("button", { name: "Approve separate agent identity" })); expect(await screen.findByRole("alert")).toHaveTextContent("mismatch"); expect(screen.queryByText(/Matching historical approval receipt/)).not.toBeInTheDocument();
});
it("does not expose approval when the server rejects another parent despite administrator hints", async () => {
  const human = agentHuman(); human.principal.global_permissions = ["system_admin"]; vi.mocked(getAuthMe).mockResolvedValue(human);
  vi.mocked(listCompanyAgents).mockRejectedValue(new CompanyAgentRequestError("Exact parent denied; no administrator override.", false));
  render(<CompanyAgentsPanel readOnly={false} initialRequest={JSON.stringify(agentEnvelope())} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("Exact parent denied"); expect(screen.queryByLabelText("Exact company agent request")).not.toBeInTheDocument(); expect(approveCompanyAgent).not.toHaveBeenCalled();
});
it("requires valid fixed expiry without extending or regenerating the request", async () => {
  const value = agentEnvelope(); value.request.expires_at = new Date(Date.now() - 1000).toISOString();
  render(<CompanyAgentsPanel readOnly={false} initialRequest={JSON.stringify(value)} />); expect(await screen.findByRole("alert")).toHaveTextContent("fixed unexpired expiry");
  expect(listCompanyAgents).not.toHaveBeenCalled(); expect(approveCompanyAgent).not.toHaveBeenCalled();
});
it("reviews and explicitly revokes only an independent agent, retaining stable receipt", async () => {
  const value = agentEnvelope(); const receipt = agentReceipt(value); vi.mocked(listCompanyAgents).mockResolvedValue({ schema_version: 1, agents: [receipt] }); await review(value);
  fireEvent.click(screen.getByRole("button", { name: `Review revoke agent ${receipt.agent_id}` })); expect(revokeCompanyAgent).not.toHaveBeenCalled();
  vi.mocked(revokeCompanyAgent).mockResolvedValue({ ...receipt, revoked_at: new Date().toISOString() }); fireEvent.click(screen.getByRole("button", { name: "Confirm separate agent revocation" }));
  await waitFor(() => expect(revokeCompanyAgent).toHaveBeenCalledWith(aid(2), aid(4), expect.any(AbortSignal))); await screen.findByText(/Separate agent revocation recorded/);
});
it("keeps an uncertain revocation on the same agent and refuses another target", async () => {
  const value = agentEnvelope(); const receipt = agentReceipt(value); vi.mocked(listCompanyAgents).mockResolvedValue({ schema_version: 1, agents: [receipt] }); await review(value);
  vi.mocked(revokeCompanyAgent).mockRejectedValueOnce(new CompanyAgentRequestError("Revocation unconfirmed", true)).mockResolvedValueOnce({ ...receipt, revoked_at: new Date().toISOString() });
  fireEvent.click(screen.getByRole("button", { name: `Review revoke agent ${receipt.agent_id}` })); fireEvent.click(screen.getByRole("button", { name: "Confirm separate agent revocation" })); await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: "Cancel agent revocation" })).toBeDisabled(); expect(screen.getByRole("button", { name: `Review revoke agent ${receipt.agent_id}` })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Retry exact agent revocation" })); await screen.findByText(/Separate agent revocation recorded/);
  expect(vi.mocked(revokeCompanyAgent).mock.calls.map(c => c.slice(0, 2))).toEqual([[aid(2), aid(4)], [aid(2), aid(4)]]);
});
it("Reader Access mounts only own Devices and never requests administrator inventories", async () => {
  render(<AccessTab project="same-slug" administrationAllowed={false} />); await screen.findByText(/Reviewing as Parent Reader/);
  for (const request of [onboardingRequest, listAuthPrincipals, listAuthMemberships, listAuthTokens]) expect(request).not.toHaveBeenCalled();
  expect(screen.queryByRole("button", { name: "Users" })).not.toBeInTheDocument(); expect(screen.queryByRole("button", { name: "Create" })).not.toBeInTheDocument();
});
it("aborts inventory on unmount and never approves a late response", async () => {
  let resolve!: (v: any) => void; vi.mocked(listCompanyAgents).mockReturnValue(new Promise(r => { resolve = r; }));
  const view = render(<CompanyAgentsPanel readOnly={false} initialRequest={JSON.stringify(agentEnvelope())} />); await waitFor(() => expect(listCompanyAgents).toHaveBeenCalledOnce());
  const signal = vi.mocked(listCompanyAgents).mock.calls[0][1]!; view.unmount(); expect(signal.aborted).toBe(true); resolve({ schema_version: 1, agents: [] }); expect(approveCompanyAgent).not.toHaveBeenCalled();
});
