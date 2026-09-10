import { useEffect, useRef, useState } from "react";

import type { ActivityEvent, MemoryEntryResponse, ProjectMemoriesResponse, ProjectOverviewResponse, StreamRequest, StreamResponse } from "../types";
import { websocketUrl } from "../utils/network";
import { useTimelineRecovery } from "./useTimelineRecovery";

interface ProjectStreamOptions {
  project: string;
  selectedMemoryId: string | null;
  wsRef: React.MutableRefObject<WebSocket | null>;
  sendStream: (request: StreamRequest, socket?: WebSocket | null) => void;
  setConnectionState: (state: "connecting" | "live" | "offline") => void;
  setStatusMessage: (message: string) => void;
  setOverview: (overview: ProjectOverviewResponse) => void;
  setProjectMemories: (memories: ProjectMemoriesResponse) => void;
  setSelectedMemory: (memory: MemoryEntryResponse | null) => void;
  addActivityEvent: (event: ActivityEvent) => void;
  recordLocalDiagnostic: (component: string, operation: string, message: string) => void;
}

export function useProjectStream({
  project,
  selectedMemoryId,
  wsRef,
  sendStream,
  setConnectionState,
  setStatusMessage,
  setOverview,
  setProjectMemories,
  setSelectedMemory,
  addActivityEvent,
  recordLocalDiagnostic,
}: ProjectStreamOptions) {
  const [generation, setGeneration] = useState(0);
  const reconnectDelay = useRef(1000);
  const callbacks = useRef({
    setOverview,
    setProjectMemories,
    setSelectedMemory,
    addActivityEvent,
    recordLocalDiagnostic,
    setStatusMessage,
  });

  useEffect(() => {
    callbacks.current = {
      setOverview,
      setProjectMemories,
      setSelectedMemory,
      addActivityEvent,
      recordLocalDiagnostic,
      setStatusMessage,
    };
  }, [addActivityEvent, recordLocalDiagnostic, setOverview, setProjectMemories, setSelectedMemory, setStatusMessage]);

  const recover = useTimelineRecovery({
    project, addActivityEvent, setStatusMessage, recordLocalDiagnostic,
    changed: () => sendStream({ type: "resync", project }),
  });

  useEffect(() => {
    let disposed = false;
    let reconnect: ReturnType<typeof setTimeout> | undefined;
    const socket = new WebSocket(websocketUrl());
    wsRef.current = socket;
    setConnectionState("connecting");

    socket.addEventListener("open", () => {
      if (disposed) return;
      reconnectDelay.current = 1000;
      setConnectionState("live");
      sendStream({ type: "subscribe_project", project }, socket);
      recover();
      if (selectedMemoryId) {
        sendStream({ type: "subscribe_memory", memory_id: selectedMemoryId }, socket);
      }
    });

    socket.addEventListener("message", (event) => {
      if (disposed) return;
      let payload: StreamResponse;
      try { payload = JSON.parse(String(event.data)) as StreamResponse; }
      catch {
        callbacks.current.recordLocalDiagnostic("websocket", "message", "Invalid stream message ignored.");
        return;
      }
      if (payload.type === "project_snapshot" || payload.type === "project_changed") {
        callbacks.current.setOverview(payload.overview);
        callbacks.current.setProjectMemories(payload.memories);
      } else if (payload.type === "project_memories") {
        callbacks.current.setProjectMemories(payload.value);
      } else if (payload.type === "overview_changed") {
        callbacks.current.setOverview(payload.overview);
      } else if (payload.type === "memory_upserted") {
        if (selectedMemoryId && payload.detail.id === selectedMemoryId) {
          callbacks.current.setSelectedMemory(payload.detail);
        }
        sendStream({ type: "project_memories", project }, socket);
      } else if (payload.type === "memory_removed") {
        if (selectedMemoryId && payload.memory_id === selectedMemoryId) {
          callbacks.current.setSelectedMemory(null);
        }
        sendStream({ type: "project_memories", project }, socket);
      } else if (payload.type === "memory_snapshot" || payload.type === "memory_changed") {
        callbacks.current.setSelectedMemory(payload.detail);
      } else if (payload.type === "activity") {
        callbacks.current.addActivityEvent(payload.event);
        recover();
      } else if (payload.type === "error") {
        callbacks.current.setStatusMessage(payload.message);
        callbacks.current.recordLocalDiagnostic("websocket", "stream", payload.message);
      }
    });

    socket.addEventListener("close", () => {
      if (disposed) return;
      setConnectionState("offline");
      callbacks.current.setStatusMessage("Live connection lost. Timeline polling continues; reconnecting automatically.");
      callbacks.current.recordLocalDiagnostic("websocket", "close", "Live connection lost.");
      reconnect = setTimeout(() => setGeneration((current) => current + 1), reconnectDelay.current);
      reconnectDelay.current = Math.min(reconnectDelay.current * 2, 30000);
    });

    socket.addEventListener("error", () => {
      if (disposed) return;
      setConnectionState("offline");
      callbacks.current.recordLocalDiagnostic("websocket", "error", "WebSocket connection failed.");
    });

    return () => {
      disposed = true;
      clearTimeout(reconnect);
      socket.close();
      if (wsRef.current === socket) wsRef.current = null;
    };
  }, [project, selectedMemoryId, sendStream, setConnectionState, wsRef, recover, generation]);
}
