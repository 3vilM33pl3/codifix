import { act, renderHook } from "@testing-library/react";
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
