import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { getActivityPage } from "../api";
import type { ActivityEvent, ActivityListResponse } from "../types";
import { useTimelineRecovery } from "./useTimelineRecovery";

vi.mock("../api", () => ({ getActivityPage: vi.fn() }));
const api = vi.mocked(getActivityPage);
const event = (seq: number) => ({ id: String(seq), seq }) as ActivityEvent;
const page = (project: string, after: number, items: ActivityEvent[] = [], more = false, gap = 0): ActivityListResponse => ({
  project, items, total_returned: items.length,
  cursor: { next_after: after, has_more: more, retention_gap: gap > 0, deleted_through: gap },
});
const options = () => ({ project: "first", addActivityEvent: vi.fn(), changed: vi.fn(), setStatusMessage: vi.fn(), recordLocalDiagnostic: vi.fn() });
afterEach(() => { vi.useRealTimers(); vi.resetAllMocks(); });

describe("committed timeline recovery", () => {
  it("accepts sparse global sequences, pages in order, and keeps its cursor across wakeups", async () => {
    const opts = options();
    api.mockResolvedValueOnce(page("first", 4, [event(4)], true))
      .mockResolvedValueOnce(page("first", 19, [event(19)]))
      .mockResolvedValue(page("first", 19));
    const hook = renderHook(() => useTimelineRecovery(opts));
    await act(async () => {});
    expect(api.mock.calls.map((call) => call[1])).toEqual([0, 4]);
    expect(opts.addActivityEvent.mock.calls.map(([e]) => e.seq)).toEqual([4, 19]);
    await act(async () => { hook.result.current(); });
    expect(api.mock.calls[2][1]).toBe(19);
    expect(opts.recordLocalDiagnostic).not.toHaveBeenCalled();
    hook.unmount();
  });

  it("polls when notifications are lost and explicitly reports retention gaps once", async () => {
    vi.useFakeTimers();
    const opts = options();
    api.mockResolvedValue(page("first", 80, [event(80)], false, 50));
    const hook = renderHook(() => useTimelineRecovery(opts));
    await act(async () => {});
    expect(opts.setStatusMessage).toHaveBeenCalledWith(expect.stringContaining("removed by retention"));
    await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
    expect(api).toHaveBeenCalledTimes(2);
    expect(api.mock.calls[1][1]).toBe(80);
    expect(opts.setStatusMessage).toHaveBeenCalledTimes(1);
    hook.unmount();
  });

  it("bounds backlog pages, retries failures, and discards a late previous-project response", async () => {
    const opts = options();
    let resolve!: (value: ActivityListResponse) => void;
    api.mockImplementationOnce(() => new Promise((done) => { resolve = done; }))
      .mockImplementation(async (project, after) => page(project, after + 1, [event(after + 1)], true));
    const hook = renderHook(({ project }) => useTimelineRecovery({ ...opts, project }), { initialProps: { project: "first" } });
    hook.rerender({ project: "second" });
    await act(async () => { resolve(page("first", 999, [event(999)])); });
    expect(opts.addActivityEvent).not.toHaveBeenCalledWith(event(999));
    expect(api).toHaveBeenCalledTimes(6); // one abandoned request plus five bounded pages
    expect(api.mock.calls[0][2]?.aborted).toBe(true);
    api.mockRejectedValueOnce(new Error("network"));
    await act(async () => { hook.result.current(); });
    expect(opts.recordLocalDiagnostic).toHaveBeenCalledWith("timeline", "recovery", expect.any(String));
    await act(async () => { hook.result.current(); });
    expect(api.mock.calls[7][1]).toBe(5); // failed page did not advance
    hook.unmount();
  });
});
