import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { supervisorPolicy, supervisorStatus } from "../../api";
import { SupervisorTab } from "./SupervisorTab";
import { dollars, type SupervisorResponse } from "./types";
vi.mock("../../api", () => ({ supervisorStatus: vi.fn(), supervisorPolicy: vi.fn() }));
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
beforeEach(() => { vi.mocked(supervisorStatus).mockReset(); vi.mocked(supervisorPolicy).mockReset(); });
afterEach(cleanup);

it("confirms exact policy terms and preserves the request ID across an uncertain retry", async () => {
  const mutable = { ...fixture, read_only: false, can_control: true };
  vi.mocked(supervisorStatus).mockResolvedValue(mutable);
  vi.mocked(supervisorPolicy).mockRejectedValueOnce(new Error("Outcome unconfirmed. Retry this same request ID.")).mockResolvedValueOnce();
  render(<SupervisorTab />);
  fireEvent.click(await screen.findByRole("button", { name: "Pause supervisor" }));
  const dialog = screen.getByRole("dialog", { name: "Confirm supervisor policy change" });
  expect(dialog).toHaveTextContent("synthetic-instance");
  expect(dialog).toHaveTextContent("synthetic-policy");
  expect(supervisorPolicy).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Refresh supervisor" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Confirm policy change" }));
  await screen.findByRole("button", { name: "Retry same request" });
  const first = vi.mocked(supervisorPolicy).mock.calls[0][0];
  expect(first).toMatchObject({ instance_id: "synthetic-instance", policy_revision: "synthetic-policy", expected_enabled: true, enabled: false });
  fireEvent.click(screen.getByRole("button", { name: "Retry same request" }));
  await waitFor(() => expect(supervisorStatus).toHaveBeenCalledTimes(2));
  expect(vi.mocked(supervisorPolicy).mock.calls[1][0]).toEqual(first);
});

it("canceling confirmation never sends a policy mutation", async () => {
  vi.mocked(supervisorStatus).mockResolvedValue({ ...fixture, read_only: false, can_control: true });
  render(<SupervisorTab />);
  fireEvent.click(await screen.findByRole("button", { name: "Pause supervisor" }));
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(supervisorStatus).toHaveBeenCalledTimes(2));
  expect(supervisorPolicy).not.toHaveBeenCalled();
});

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

it("shows historical deterministic checks without claiming current health or model activation", async () => {
  vi.mocked(supervisorStatus).mockResolvedValue({ ...fixture, deterministic_checks_configured: true, snapshot: {
    ...fixture.snapshot!, last_health_observation: { action_id: "health-action", policy_revision: "old-policy", observed_at: "2026-09-17T10:00:00Z", next_due_at: "2026-09-17T10:05:00Z", project_count: "7", canonical_count: "100", blocked_jobs: "2", failed_jobs: "3", uncertain_actions: "1" },
  }});
  render(<SupervisorTab />);
  const observation = await screen.findByRole("region", { name: "Last operational observation" });
  expect(observation).toHaveTextContent("old-policy");
  expect(observation).toHaveTextContent("2 blocked jobs; 3 failed jobs; 1 uncertain actions");
  expect(observation).toHaveTextContent("does not prove current availability");
  expect(screen.getByText(/Deterministic operational checks:/)).toHaveTextContent("do not enable Pi or model calls");
});
