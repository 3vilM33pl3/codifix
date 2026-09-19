import { render, screen, act, cleanup } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { GatewayHealth } from "./GatewayHealth";
import { GrantHealth } from "../supervisor/GrantHealth";
import type { GatewayHealth as Health } from "./types";
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
it("does not label a blocked publication healthy even with a usable token", () => {
  const now = Date.now() / 1000;
  const health: Health = { credentials: { observed_at: now, mode: "rotating", state: "usable", publication_usable: true, generation: 2, expires_at: now + 300, maintenance_state: "healthy", maintenance_observed_at: now, last_success: now, retry_at: null, tls_expires_at: null, reason: null }, pending: { sequence: 10, attempts: 1, blocked: true, leased: false, due: false, failure: "invalid_receipt" } };
  render(<GatewayHealth health={health} />);
  expect(screen.getByRole("status")).toHaveTextContent("Publication blocked");
  expect(screen.getByRole("status")).not.toHaveTextContent("eligible for publication attempts");
});
it("ages maintenance health independently from a fresh gateway observation", () => {
  vi.useFakeTimers(); vi.setSystemTime(new Date("2026-09-19T12:00:00Z"));
  const now = Date.now() / 1000;
  render(<GatewayHealth health={{ credentials: { observed_at: now, mode: "rotating", state: "usable", publication_usable: true, generation: 2, expires_at: now + 300, maintenance_state: "healthy", maintenance_observed_at: now - 175, last_success: now - 175, retry_at: null, tls_expires_at: null, reason: null }, pending: null }} />);
  expect(screen.getByText(/Renewal: healthy/)).toBeInTheDocument();
  act(() => vi.advanceTimersByTime(10000));
  expect(screen.queryByText(/Renewal: healthy/)).toBeNull();
  expect(screen.getByText(/maintenance observation stale/)).toBeInTheDocument();
});
