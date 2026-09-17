import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { supervisorStatus } from "../../api";
import { SupervisorTab } from "./SupervisorTab";
import { dollars, type SupervisorResponse } from "./types";
vi.mock("../../api", () => ({ supervisorStatus: vi.fn() }));
const fixture: SupervisorResponse = {
  read_only: true, execution: "not_integrated", snapshot: {
    instance_id: "synthetic-instance", area: "workshop", observed_at: "2026-09-17T12:00:00Z",
    policy: { revision: "synthetic-policy", enabled: true, revoked: false, model: "gpt-5.6-sol", daily_usd_micros: "2500000", daily_calls: 20 },
    budget: { day: "2026-09-17", charged_usd_micros: "12500", reserved_usd_micros: "45000", calls: 1 },
    action_counts: [{ state: "uncertain", count: "1" }],
    journal: [{ sequence: "9007199254740993", action_id: "synthetic-action", attempt_id: "synthetic-attempt", policy_revision: "synthetic-policy", event: "uncertain", recorded_at: "2026-09-17T12:00:00Z" }],
    next_after: "9007199254740993", has_more: true,
  },
};
beforeEach(() => { vi.mocked(supervisorStatus).mockReset(); });
afterEach(cleanup);

it("shows actual policy and reserved charges without claiming an active agent", async () => {
  vi.mocked(supervisorStatus).mockResolvedValue(fixture);
  render(<SupervisorTab />);
  await screen.findByText("Workshop");
  expect(screen.getByRole("status")).toHaveTextContent("execution is not integrated");
  expect(screen.getByText(/UTC accounting day/)).toHaveTextContent("charged $0.012500; reserved or uncertain $0.045000");
  expect(screen.getByText(/Policy: enabled/)).toHaveTextContent("worker activation is separate");
  expect(screen.queryByRole("button", { name: /enable|run|pause|approve/i })).toBeNull();
  expect(screen.getByText("Outcome uncertain — reconciliation required")).toBeVisible();
});
it("keeps large sequence numbers exact across pagination", async () => {
  vi.mocked(supervisorStatus).mockResolvedValue(fixture);
  render(<SupervisorTab />);
  fireEvent.click(await screen.findByRole("button", { name: "Next history page" }));
  await waitFor(() => expect(vi.mocked(supervisorStatus).mock.calls[1][0]).toBe("9007199254740993"));
});
it("distinguishes an unconfigured instance from an empty history", async () => {
  vi.mocked(supervisorStatus).mockResolvedValue({ ...fixture, snapshot: null });
  render(<SupervisorTab />);
  await screen.findByText(/No durable supervisor identity/);
  expect(screen.queryByText("No newer history entries on this page.")).toBeNull();
});
it("clears old results before a failed refresh and cancels prior requests", async () => {
  vi.mocked(supervisorStatus).mockResolvedValueOnce(fixture).mockRejectedValueOnce(new Error("403 denied"));
  render(<SupervisorTab />);
  await screen.findByText("Workshop");
  fireEvent.click(screen.getByRole("button", { name: "Refresh supervisor" }));
  expect(screen.queryByText("Workshop")).toBeNull();
  expect(await screen.findByRole("alert")).toHaveTextContent("Forbidden");
  expect(vi.mocked(supervisorStatus).mock.calls[0][1]?.aborted).toBe(true);
});
it.each([["401", "Signed out"], ["403", "Forbidden"], ["409", "Identity mismatch"], ["503", "Offline"]])("distinguishes %s without displaying raw diagnostics", async (code, expected) => {
  vi.mocked(supervisorStatus).mockRejectedValue(new Error(`${code} PRIVATE_PROVIDER_BODY`));
  render(<SupervisorTab />);
  expect(await screen.findByRole("alert")).toHaveTextContent(expected);
  expect(screen.queryByText(/PRIVATE_PROVIDER_BODY/)).toBeNull();
});
it("formats exact millionths without floating point rounding", () => {
  expect(dollars("1")).toBe("$0.000001");
  expect(dollars("9007199254740993123456")).toBe("$9007199254740993.123456");
  expect(dollars("-1")).toBe("unavailable");
});
