import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import App from "../../App";
import { useAppShell } from "../../hooks/useAppShell";
vi.mock("../../hooks/useAppShell", () => ({ useAppShell: vi.fn() }));
vi.mock("../../api", async (importOriginal) => ({ ...(await importOriginal<typeof import("../../api")>()), getAuthMe: vi.fn().mockRejectedValue(new Error("401")) }));
afterEach(() => { cleanup(); window.history.replaceState({}, "", "/"); });
it("opens the device landing without mounting project storage or forwarding unrelated credential parameters", async () => {
  window.history.replaceState({}, "", "/?device_code=ABCD-1234&poll_secret=must-not-forward&bearer=must-not-forward");
  const storage = vi.spyOn(Storage.prototype, "setItem");
  render(<App />);
  expect(await screen.findByRole("link", { name: "Sign in to review this device" })).toHaveAttribute("href", "/v1/auth/login?return_to=%2F%3Fdevice_code%3DABCD-1234");
  expect(useAppShell).not.toHaveBeenCalled(); expect(storage).not.toHaveBeenCalled();
  storage.mockRestore();
});
