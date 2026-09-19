import { render, screen, act, cleanup } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { GatewayHealth } from "./GatewayHealth";
import { GrantHealth } from "../supervisor/GrantHealth";
afterEach(() => { cleanup(); vi.useRealTimers(); });
it("does not claim health when status is missing", () => {
  render(<GatewayHealth health={null} />);
  expect(screen.getByRole("status")).toHaveTextContent("publication health is unknown");
});
it("ages successful observations without waiting for another request", () => {
  vi.useFakeTimers(); vi.setSystemTime(new Date("2026-09-19T12:00:00Z"));
  const now = Date.now() / 1000;
  render(<GatewayHealth health={{ credentials: { observed_at: now, mode: "rotating", state: "usable", publication_usable: true, generation: 2, expires_at: now + 300, maintenance_state: "healthy", maintenance_observed_at: now, last_success: now, retry_at: null, tls_expires_at: now + 600, reason: null }, pending: null }} />);
  expect(screen.getByRole("status")).toHaveTextContent("eligible for publication attempts");
  act(() => vi.advanceTimersByTime(130000));
  expect(screen.getByRole("status")).toHaveTextContent("Observation stale");
});
it("an expired grant is never shown as a valid time window", () => {
  vi.useFakeTimers(); vi.setSystemTime(new Date("2026-09-19T12:00:00Z"));
  render(<GrantHealth grant={{ observed_at: new Date().toISOString(), expires_at: "2026-09-19T11:59:00Z", state: "valid_window", automatic_renewal: false }} />);
  expect(screen.getByRole("status")).toHaveTextContent("Expired");
  expect(screen.getByText(/No automatic renewal/)).toBeInTheDocument();
});
