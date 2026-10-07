import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { openLocalAdministration } from "../../api";
import type { AuthMeResponse } from "../../types";
import { LocalAdministrationButton } from "./LocalAdministrationButton";

vi.mock("../../api", () => ({ openLocalAdministration: vi.fn() }));
const reader: AuthMeResponse = { authenticated: true, mode: "single_user", read_only: false,
  principal: { id: "owner", kind: "internal", display_name: "local owner", groups: [],
    global_role: "reader", global_permissions: ["memory_read"], projects: [] } };
beforeEach(() => vi.resetAllMocks());
afterEach(cleanup);

it("requires an explicit click and opens only with the returned server identity", async () => {
  const admin = { ...reader, principal: { ...reader.principal, global_role: "admin" as const, global_permissions: ["system_admin"] } };
  vi.mocked(openLocalAdministration).mockResolvedValue(admin);
  const opened = vi.fn();
  render(<LocalAdministrationButton auth={reader} onOpened={opened} />);
  expect(openLocalAdministration).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Open administration" }));
  await waitFor(() => expect(opened).toHaveBeenCalledWith(admin));
});

it("shows a sanitized failure without opening administration and allows retry", async () => {
  vi.mocked(openLocalAdministration).mockRejectedValue(new Error("private response detail"));
  const opened = vi.fn();
  render(<LocalAdministrationButton auth={reader} onOpened={opened} />);
  fireEvent.click(screen.getByRole("button", { name: "Open administration" }));
  expect(await screen.findByRole("alert")).not.toHaveTextContent("private response detail");
  expect(opened).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Open administration" })).toBeEnabled();
});

it.each([
  { ...reader, read_only: true },
  { ...reader, mode: "multi_user" as const },
  { ...reader, principal: { ...reader.principal, kind: "service_token" as const } },
])("does not offer local login outside writable single-user owner access", (auth) => {
  render(<LocalAdministrationButton auth={auth} onOpened={vi.fn()} />);
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
  expect(openLocalAdministration).not.toHaveBeenCalled();
});
