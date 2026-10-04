import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { federationRequest } from "../../api";
import { AgentExchangesPanel } from "./AgentExchangesPanel";

vi.mock("../../api", () => ({ federationRequest: vi.fn() }));
afterEach(() => { cleanup(); vi.mocked(federationRequest).mockReset(); });

it("shows metadata only and clears stale status when access is revoked", async () => {
  vi.mocked(federationRequest).mockResolvedValueOnce([{
    request_id: "one", recipient: "Workshop", created_at: "2026-10-04T00:00:00Z",
    question_status: "published", answer_status: null,
    question: "PRIVATE QUESTION", answer: "PRIVATE ANSWER",
  }]).mockRejectedValueOnce(new Error("403 revoked"));
  render(<AgentExchangesPanel project="homelab" space="private-space" />);
  fireEvent.click(screen.getByText("Refresh agent exchanges"));
  expect((await screen.findByText("one")).closest("li")).toHaveTextContent("answer unknown");
  expect(screen.queryByText("PRIVATE QUESTION")).toBeNull();
  expect(screen.queryByText("PRIVATE ANSWER")).toBeNull();
  expect(vi.mocked(federationRequest).mock.calls[0].slice(0, 3)).toEqual(["homelab", "private-space", "/agent-questions"]);
  fireEvent.click(screen.getByText("Refresh agent exchanges"));
  expect(await screen.findByRole("alert")).toHaveTextContent("403 revoked");
  expect(screen.queryByText("one")).toBeNull();
});
