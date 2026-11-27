// Only review.html imports this module. Vite's production entry is index.html.
import { mockIPC } from "@tauri-apps/api/mocks";
import { auth, fixture } from "./fixtures";
if (!import.meta.env.DEV)
  throw new Error("The review fixture is development-only.");
localStorage.setItem(
  "codifix.desktop.preferences",
  JSON.stringify({
    theme: "light",
    endpoint: "http://127.0.0.1:4250",
    connectionName: "Workshop · Review fixture",
    notifications: false,
  }),
);
localStorage.setItem(
  "codifix.desktop.projects",
  JSON.stringify([
    { project: "codifix", endpoint: "http://127.0.0.1:4250", path: null },
  ]),
);
mockIPC((cmd, payload) => {
  if (cmd === "connect_service") return auth;
  if (cmd === "api_request") return fixture((payload as { path: string }).path);
  throw new Error(
    "Native actions are unavailable in this illustrative review fixture.",
  );
});
await import("../main");
