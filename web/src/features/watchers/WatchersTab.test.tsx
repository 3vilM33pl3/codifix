import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { overviewFixture } from "../../test/fixtures";
import { WatchersTab } from "./WatchersTab";
import type { WatcherPresence } from "../../types";

describe("WatchersTab", () => {
  afterEach(cleanup);
  const watcher: WatcherPresence = {
    watcher_id: "watcher-1", project: "memory", repo_root: "/repo", hostname: "monolith",
    host_service_id: "service-1", pid: 42, mode: "suggest", managed_by_service: true,
    health: "healthy", started_at: "2026-10-08T12:00:00Z", last_heartbeat_at: "2026-10-08T12:01:00Z",
    agent_cli: "codex", agent_session_id: "session-1", agent_pid: 12, agent_started_at: null,
    last_restart_attempt_at: null, restart_attempt_count: 0,
  };

  it.each([ ["desktop", "Desktop session"], ["cli", "CLI session"], [undefined, "Session type unknown"] ] as const)(
    "labels %s sessions without guessing older metadata", (kind, label) => {
      render(<WatchersTab project="memory" overview={{ ...overviewFixture, watchers: {
        active_count: 1, unhealthy_count: 0, stale_after_seconds: 90, last_heartbeat_at: watcher.last_heartbeat_at,
        watchers: [{ ...watcher, agent_session_kind: kind }],
      } }} />);
      expect(screen.getByText(label)).toBeInTheDocument();
      expect(screen.getByText(/Monitoring codex/)).toBeInTheDocument();
      if (kind === "desktop") expect(screen.getByText(/desktop host pid 12/)).toBeInTheDocument();
    },
  );
  it("renders the no watcher state", () => {
    render(<WatchersTab overview={overviewFixture} project="memory" />);

    expect(screen.getByRole("heading", { name: "Watcher presence" })).toBeInTheDocument();
    expect(screen.getByText("No watcher presence data.")).toBeInTheDocument();
  });
});
