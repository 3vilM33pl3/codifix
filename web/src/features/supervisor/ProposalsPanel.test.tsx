import { StrictMode } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { supervisorPrivate, supervisorReview, supervisorQuestions, supervisorQuestionSubmit, type SupervisorProposalDetail } from "../../api";
import { ProposalsPanel } from "./ProposalsPanel";
vi.mock("../../api", () => ({ supervisorPrivate: vi.fn(), supervisorReview: vi.fn(), supervisorQuestions: vi.fn(), supervisorQuestionSubmit: vi.fn() }));
const detail: SupervisorProposalDetail = { project_id: "project", action_id: "action", proposal_digest: "a".repeat(64), question: "Private question", summary: "<script>untrusted text</script>", proposal: "ask_human", citations: [{ index: 0, canonical_id: "canonical", version_id: "exact-version", version: 1, evidence_digest: "b".repeat(64) }], created_at: "2026-09-18T12:00:00Z", review: null, can_review: true };
beforeEach(() => { vi.mocked(supervisorPrivate).mockReset(); vi.mocked(supervisorReview).mockReset(); vi.mocked(supervisorQuestions).mockReset(); vi.mocked(supervisorQuestionSubmit).mockReset(); });
afterEach(cleanup);
async function open() {
  vi.mocked(supervisorPrivate).mockResolvedValueOnce({ actions: ["action"], next_after: null }).mockResolvedValueOnce(detail);
  fireEvent.click(screen.getByRole("button", { name: "Load saved proposals" }));
  fireEvent.click(await screen.findByRole("button", { name: "Open proposal action" }));
  await screen.findByText("Private question");
}
it("loads private results only explicitly, with safe text rendering and StrictMode lifecycle", async () => {
  render(<StrictMode><ProposalsPanel project="memory" /></StrictMode>);
  expect(supervisorPrivate).not.toHaveBeenCalled();
  await open();
  expect(screen.getByText("<script>untrusted text</script>")).toBeVisible();
  expect(document.querySelector("article script")).toBeNull();
  expect(screen.getByText("exact-version")).toBeVisible();
  expect(vi.mocked(supervisorPrivate).mock.calls[0][2]?.aborted).toBe(false);
});
it("requires confirmation and retries an uncertain review with unchanged terms", async () => {
  render(<ProposalsPanel project="memory" />); await open();
  vi.mocked(supervisorReview).mockRejectedValueOnce(new Error("Outcome unconfirmed"));
  fireEvent.click(screen.getByRole("button", { name: "Approve proposal" }));
  expect(screen.getByRole("dialog")).toHaveTextContent("No installation, source edit, model call or execution permission");
  expect(supervisorReview).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Confirm review" }));
  await screen.findByRole("button", { name: "Retry same review" });
  const request = vi.mocked(supervisorReview).mock.calls[0][2];
  vi.mocked(supervisorReview).mockResolvedValueOnce({ ...request, action_id: "action", reviewer_id: "human", recorded_at: "now" });
  fireEvent.click(screen.getByRole("button", { name: "Retry same review" }));
  await screen.findByText(/Review recorded: approve/);
  expect(vi.mocked(supervisorReview).mock.calls[1][2]).toEqual(request);
  expect(screen.queryByRole("button", { name: "Approve proposal" })).toBeNull();
});
it("clears private data on project change and on denied refresh", async () => {
  const view = render(<ProposalsPanel key="memory" project="memory" />); await open();
  view.rerender(<ProposalsPanel key="homelab" project="homelab" />);
  expect(screen.queryByText("Private question")).toBeNull();
  expect(vi.mocked(supervisorPrivate).mock.calls[1][2]?.aborted).toBe(true);
  vi.mocked(supervisorPrivate).mockRejectedValueOnce(new Error("Access denied"));
  fireEvent.click(screen.getByRole("button", { name: "Load saved proposals" }));
  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Access denied"));
  expect(screen.queryByText("exact-version")).toBeNull();
});

it("mounts the question form inside proposals and retains exact consent during saved-result inspection", async () => {
  render(<StrictMode><ProposalsPanel project="memory" /></StrictMode>);
  expect(screen.getByRole("textbox", { name: "Exact question" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Prepare question" })).toBeDisabled();
  expect(supervisorQuestions).not.toHaveBeenCalled();
  vi.mocked(supervisorQuestions).mockResolvedValueOnce({ question: "New explicit question", profile_digest: "c".repeat(64), request_digest: "d".repeat(64), destination: "openai", model: "gpt-5.6-sol", source_count: 3, daily_call_limit: 20, daily_usd_micros: "2500000" });
  fireEvent.change(screen.getByRole("textbox", { name: "Exact question" }), { target: { value: "New explicit question" } });
  fireEvent.click(screen.getByRole("button", { name: "Prepare question" }));
  const confirmation = await screen.findByRole("dialog", { name: "Confirm paid supervisor question" });
  const exactConsent = confirmation.textContent;
  await open();
  expect(screen.getByRole("dialog", { name: "Confirm paid supervisor question" }).textContent).toBe(exactConsent);
  expect(screen.getByRole("textbox", { name: "Exact question" })).toHaveValue("New explicit question");
  expect(supervisorQuestionSubmit).not.toHaveBeenCalled();
});
