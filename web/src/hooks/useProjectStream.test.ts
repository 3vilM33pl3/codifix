import { act, renderHook } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { getActivityPage } from "../api";
import { useProjectStream } from "./useProjectStream";

vi.mock("../api", () => ({ getActivityPage: vi.fn() }));
class FakeSocket extends EventTarget {
  static OPEN = 1;
  static sockets: FakeSocket[] = [];
  readyState = 0;
  constructor() { super(); FakeSocket.sockets.push(this); }
  open() { this.readyState = 1; this.dispatchEvent(new Event("open")); }
  close() { this.readyState = 3; this.dispatchEvent(new Event("close")); }
  message(data: string) { this.dispatchEvent(new MessageEvent("message", { data })); }
}
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.resetAllMocks(); FakeSocket.sockets = []; });

function renderStatusHook(initialMessage: string) {
  vi.useFakeTimers();
  vi.stubGlobal("WebSocket", FakeSocket);
  vi.mocked(getActivityPage).mockImplementation(async (project, after) => ({
    project, total_returned: 0, items: [], cursor: { next_after: after, has_more: false, retention_gap: false, deleted_through: 0 },
  }));
  const opts = {
    project: "synthetic", selectedMemoryId: null, wsRef: { current: null }, sendStream: vi.fn(),
    setOverview: vi.fn(), setProjectMemories: vi.fn(), setSelectedMemory: vi.fn(),
    addActivityEvent: vi.fn(), recordLocalDiagnostic: vi.fn(),
  };
  return renderHook(() => {
    const [statusMessage, setStatusMessage] = useState(initialMessage);
    const [connectionState, setConnectionState] = useState<"connecting" | "live" | "offline">("connecting");
    useProjectStream({ ...opts, setStatusMessage, setConnectionState });
    return { statusMessage, setStatusMessage, connectionState };
  });
}

it("preserves initial status and replaces only its stale loss notice after reconnect", async () => {
  const hook = renderStatusHook("Publication approval is still required.");
  await act(async () => { FakeSocket.sockets[0].open(); });
  expect(hook.result.current.statusMessage).toBe("Publication approval is still required.");
  await act(async () => { FakeSocket.sockets[0].close(); });
  expect(hook.result.current.statusMessage).toContain("Live connection lost.");
  expect(hook.result.current.connectionState).toBe("offline");
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  await act(async () => { FakeSocket.sockets[1].open(); });
  expect(hook.result.current.connectionState).toBe("live");
  expect(hook.result.current.statusMessage).toBe("Live connection restored.");
  hook.unmount();
});

it("does not overwrite a concurrent operation status when reconnect opens", async () => {
  const hook = renderStatusHook("Ready.");
  await act(async () => { FakeSocket.sockets[0].open(); });
  await act(async () => { FakeSocket.sockets[0].close(); await vi.advanceTimersByTimeAsync(1000); });
  // Both updates are queued in one batch: a captured status value would be stale.
  await act(async () => {
    hook.result.current.setStatusMessage("Publication denied; review current permissions.");
    FakeSocket.sockets[1].open();
  });
  expect(hook.result.current.connectionState).toBe("live");
  expect(hook.result.current.statusMessage).toBe("Publication denied; review current permissions.");
  await act(async () => { FakeSocket.sockets[1].close(); await vi.advanceTimersByTimeAsync(1000); });
  await act(async () => {
    FakeSocket.sockets[2].open();
    hook.result.current.setStatusMessage("A newer review still needs confirmation.");
  });
  expect(hook.result.current.statusMessage).toBe("A newer review still needs confirmation.");
  hook.unmount();
});

it("preserves a newer stream error instead of claiming reconnect resolved it", async () => {
  const hook = renderStatusHook("Ready.");
  await act(async () => { FakeSocket.sockets[0].open(); });
  await act(async () => { FakeSocket.sockets[0].close(); await vi.advanceTimersByTimeAsync(1000); });
  await act(async () => {
    FakeSocket.sockets[1].message(JSON.stringify({ type: "error", message: "Project access denied." }));
    FakeSocket.sockets[1].open();
  });
  expect(hook.result.current.connectionState).toBe("live");
  expect(hook.result.current.statusMessage).toBe("Project access denied.");
  hook.unmount();
});

it("reconnects without advancing recovery past broadcasts and ignores stale project messages", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("WebSocket", FakeSocket);
  vi.mocked(getActivityPage).mockImplementation(async (project, after) => ({
    project, total_returned: 0, items: [], cursor: { next_after: after, has_more: false, retention_gap: false, deleted_through: 0 },
  }));
  const opts = {
    selectedMemoryId: null, wsRef: { current: null }, sendStream: vi.fn(),
    setConnectionState: vi.fn(), setStatusMessage: vi.fn(), setOverview: vi.fn(),
    setProjectMemories: vi.fn(), setSelectedMemory: vi.fn(), addActivityEvent: vi.fn(), recordLocalDiagnostic: vi.fn(),
  };
  const hook = renderHook(({ project }) => useProjectStream({ ...opts, project }), { initialProps: { project: "first" } });
  await act(async () => { FakeSocket.sockets[0].open(); });
  await act(async () => { FakeSocket.sockets[0].message(JSON.stringify({ type: "activity", event: { id: "hint", seq: 999 } })); });
  expect(vi.mocked(getActivityPage).mock.calls.every((call) => call[1] === 0)).toBe(true);
  expect(opts.sendStream).not.toHaveBeenCalledWith({ type: "resync", project: "first" });
  await act(async () => { FakeSocket.sockets[0].close(); await vi.advanceTimersByTimeAsync(1000); });
  expect(FakeSocket.sockets).toHaveLength(2);
  await act(async () => { FakeSocket.sockets[1].open(); });
  expect(opts.setConnectionState).toHaveBeenLastCalledWith("live");
  hook.rerender({ project: "second" });
  await act(async () => { FakeSocket.sockets[1].message(JSON.stringify({ type: "activity", event: { id: "late" } })); });
  expect(opts.addActivityEvent).not.toHaveBeenCalledWith({ id: "late" });
  await act(async () => { FakeSocket.sockets[2].message("not json"); });
  expect(opts.recordLocalDiagnostic).toHaveBeenCalledWith("websocket", "message", "Invalid stream message ignored.");
  hook.unmount();
  await vi.advanceTimersByTimeAsync(30000);
  expect(FakeSocket.sockets).toHaveLength(3);
});
