import { fireEvent, render, screen, cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AreaRuntimeStatus } from "./AreaRuntimeStatus";

afterEach(cleanup);
describe("AreaRuntimeStatus", () => {
  const now = Date.parse("2026-09-29T20:00:00Z");
  const status = { generated_at: new Date(now).toISOString(),
    area: { name: "workshop", instance_id: "22222222-2222-4222-8222-222222222222" },
    federation_configured: true };
  it("shows exact identity and opens status without claiming connection health", () => {
    const open = vi.fn();
    render(<AreaRuntimeStatus status={status} now={now} onOpenFederation={open} />);
    expect(screen.getByText("Area workshop")).toBeInTheDocument();
    expect(screen.getByText(`Instance ${status.area.instance_id}`)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Federation configured" }));
    expect(open).toHaveBeenCalledOnce();
  });
  it.each([null, { ...status, generated_at: "invalid" },
    { ...status, generated_at: new Date(now - 16_000).toISOString() }])("does not present missing or stale status as configured", value => {
    render(<AreaRuntimeStatus status={value} now={now} onOpenFederation={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Federation unknown" })).toBeInTheDocument();
  });
});
