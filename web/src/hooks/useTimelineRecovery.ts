import { useCallback, useEffect, useRef } from "react";

import { getActivityPage } from "../api";
import type { ActivityEvent } from "../types";

interface Options {
  project: string;
  addActivityEvent: (event: ActivityEvent) => void;
  changed: () => void;
  setStatusMessage: (message: string) => void;
  recordLocalDiagnostic: (component: string, operation: string, message: string) => void;
}

/** Notifications are hints. Only committed pages advance this cursor, never a
 * broadcast sequence: global sequence gaps are normal within one project. */
export function useTimelineRecovery(options: Options) {
  const callbacks = useRef(options);
  const wake = useRef(() => {});
  useEffect(() => { callbacks.current = options; });
  useEffect(() => {
    const abort = new AbortController();
    let disposed = false;
    let active = false;
    let cursor = 0;
    let reportedGap = 0;
    async function recover() {
      if (disposed || active) return;
      active = true;
      let changed = false;
      try {
        // Large backlogs continue on the next poll, never an unbounded loop.
        for (let pages = 0; pages < 5; pages += 1) {
          const page = await getActivityPage(options.project, cursor, abort.signal);
          if (disposed) return;
          if (page.project !== options.project || !page.cursor) throw new Error("Invalid timeline page");
          if (page.cursor.retention_gap && page.cursor.deleted_through > reportedGap) {
            reportedGap = page.cursor.deleted_through;
            const message = "Some activity history was removed by retention. Current project data remains available; the missing history cannot be reconstructed.";
            callbacks.current.setStatusMessage(message);
            callbacks.current.recordLocalDiagnostic("timeline", "retention_gap", message);
            changed = true;
          }
          for (const event of page.items) callbacks.current.addActivityEvent(event);
          changed ||= page.items.length > 0;
          if (page.cursor.next_after < cursor || (page.cursor.has_more && page.cursor.next_after === cursor)) throw new Error("Timeline cursor did not advance");
          cursor = page.cursor.next_after;
          if (!page.cursor.has_more) break;
        }
        if (changed) callbacks.current.changed();
      } catch {
        if (!disposed) callbacks.current.recordLocalDiagnostic("timeline", "recovery", "Timeline recovery is unavailable; polling will retry.");
      } finally {
        active = false;
      }
    }
    wake.current = () => { void recover(); };
    void recover();
    const poll = setInterval(() => { void recover(); }, 5000);
    return () => {
      disposed = true;
      abort.abort();
      clearInterval(poll);
      wake.current = () => {};
    };
  }, [options.project]);
  return useCallback(() => wake.current(), []);
}
