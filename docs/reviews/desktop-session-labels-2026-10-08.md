# Desktop session labels in Web and TUI

Implemented and installed on Monolith at Olivier's request. Watcher heartbeats
and presence now carry optional `agent_session_kind` (`desktop` or `cli`). The
watcher checks its live owner's PID/start time and command; Codex `app-server`
is desktop, supported CLI owners are CLI, and missing/unsupported metadata is
unknown. This is monitoring metadata, not authentication or a claim that an
agent is executing a model call at that moment.

Web watcher cards display a Desktop session badge and identify the desktop host
PID separately from the conversation ID. TUI monitoring labels precede long
repository and watcher identifiers so they remain visible in a small terminal.
Older payloads decode unchanged and display Session type unknown.

## Observed delivery

- Live General → Soundmind → Watchers shows Desktop session, Monitoring codex,
  desktop host PID 32981, one active watcher and zero unhealthy.
- A real installed-command PTY run showed `Monitoring: codex — Desktop session`
  on the initial Watchers screen without scrolling; the test TUI then exited.
- Workshop, Homelab, General and the dev preview have healthy databases after
  updating the backend. Workshop federation remains through sequence 23 with
  zero sync errors/failures and no queued or blocked intents.
- No migration, gateway/PDS change, paid call, permission change or CA install.

Deployment and previous binary/Web/launcher material are preserved under
`~/.local/state/memory-session-ui-20261008`.

The Debian-compatible backend and initial watcher hotfix binary SHA-256 is
`d9a97013c1fd051895012d74fab848aaecca4298e85b693b2cc7f8cf25883a20`.
The final native Monolith full-feature CLI/TUI binary (with the higher label
placement) is
`14a8e00b3f338dbf1da4e904717809be7530cec76437871753ad6973ca962b74`.
The normal user launcher selects the latter. The watcher-manager override uses
the same client path; already-running manager/watchers retain the compatible
binary they started with until restarted. Backend containers retain the former.
Frozen rc.5 packages, `/usr/bin/memory`, and Trust are not rewritten by this local
hotfix. Use the normal `memory` launcher for the updated TUI.

The user-service override, original launcher, binaries and assets have recoverable
copies. Reverting this display change requires only software/asset replacement
and relevant service restarts; it must not restore a stale database snapshot.

## Checks

- 29 agent-discovery, seven shared-record and 21 watcher unit tests passed.
- Focused service heartbeat registry and TUI label tests passed.
- 274 Web tests and the Web production build passed.
- Targeted record/watcher, service and lean CLI Clippy passed with warnings denied.
- Formatting, diff and boundary checks passed.
- Native full CLI and Debian-compatible full release builds passed.

The combined discovery test exposed a parallel-fixture race in an assertion
that assumed only one process-open rollout. The test now verifies its fixture is
among the open paths and that the selected CLI path belongs to that set. No
production discovery behavior was changed by that test correction.
