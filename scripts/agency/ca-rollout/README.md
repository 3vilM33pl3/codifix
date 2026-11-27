# Supervise CA companion and TUI releases

Use these Linux tools from an operator-reviewed source revision. Run the CA
companion under a dedicated unprivileged systemd account. Keep its executable
and configuration in root-owned directories, and give this service its own
executable path. Its main process must run `codifix-agency --config ABSOLUTE_CONFIG
serve` directly. Install `console.py`, `enroll-tui.py`, `tui-supervisor.py`,
`tui.py`, and `installer-journal.py` together under a root-owned directory such
as `/usr/local/libexec/codifix-ca`. Keep the files and their parent directories
unwritable by the worker and TUI operator.

Enroll the publisher's private identity in the existing companion configuration.
Grant `upgrade_candidate` to that peer for exactly one installation resource
whose ID is the CA companion instance UUID. Use `installation` for the companion
executable and a separate mandate for `tui_installation`. Set finite budgets and
an expiry. The console verifies this binding against the running service and
installs only after the unprivileged supervisor admits a matching file effect.

## Companion executable

Create a root-owned settings file with mode `0600`. Replace the example IDs,
paths, peer and health URLs with your reviewed values.

```json
{
  "unit": "codifix-companion.service",
  "worker_binary": "/opt/codifix-ca/bin/codifix-agency",
  "worker_config": "/etc/codifix-ca/worker.json",
  "instance_id": "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa",
  "publisher_peer": "publisher",
  "publisher_instance_id": "bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb",
  "publisher_did": "did:web:publisher.example.test",
  "mandate_id": "cccccccc-cccc-4ccc-cccc-cccccccccccc",
  "specialist_id": "companion-installer",
  "companion_health": "http://127.0.0.1:8787/health",
  "bundle": "/srv/codifix-ca/candidate",
  "openbao_health": "https://bao.example.test/v1/sys/health"
}
```

Place the reviewed `codifix-agency`, `tested.json` and both named test logs in
the protected bundle. Obtain the bytes through a trusted release path and
review the receipt independently on the CA before running the console. Make
trusted TLS roots available to the worker. The console verifies artifact and
log digests, then runs `upgrade-once` under the same Unix identity and database
binding as the worker. Only the admitted callback can replace the executable
and restart the named unit. The Rust supervisor handles health and the private
return-message verification. Run once as root while the publisher waits:

```sh
/usr/bin/python3 -I /usr/local/libexec/codifix-ca/console.py \
  --settings /etc/codifix-ca/companion-rollout.json
```

## TUI executable

Seed a dedicated root-owned TUI path with the existing working client. Configure
your root-owned launcher to execute that path, preserving its normal arguments
and observer configuration. Install `rclone` and `tmux`. Configure one read-only
S3 `release` remote in a root-only rclone configuration. Use path-style requests,
set its endpoint to the reviewed HTTPS object origin, and give the credential
read access only to the CA TUI release objects. The object URL must be canonical
and use `/codifix-releases/ca/VERSION/tui-companion-footer-SUFFIX` on that origin.

Create a separate settings file with the same common fields as the companion
example. Set `mandate_id` and `specialist_id` to the TUI values, remove `bundle`
and `openbao_health`, and add these fields:

```json
{
  "installed_tui": "/opt/codifix-ca/bin/codifix-tui",
  "launcher": "/usr/local/bin/launch-codifix-tui",
  "operator": "terminal-user",
  "rclone_config": "/etc/codifix-ca/reader.conf",
  "object_origin": "https://objects.example.test/"
}
```

Run as root. To poll, have an operator-owned systemd oneshot execute the same
command with a timer. Enable or stop that timer through your usual administration
procedure.

```sh
/usr/bin/python3 -I /usr/local/libexec/codifix-ca/enroll-tui.py \
  --supervise --settings /etc/codifix-ca/tui-rollout.json
```

The worker fetches into disposable staging. The root broker accepts callbacks
only from the supervised worker's child, rechecks the exact pending effect and
artifact digest, and can replace the TUI once per invocation. The TUI update
keeps the companion process running. Verification runs the installed executable
and the normal launcher under the configured TUI operator. Look for the fresh
companion footer and the private peer acknowledgement in the rollout result.

Use the read-only journal utility to inspect the installer service's status and
bounded journal labels. Inspect detailed logs locally through your usual secure
operator process.

```sh
/usr/bin/python3 -I /usr/local/libexec/codifix-ca/installer-journal.py \
  --settings /etc/codifix-ca/tui-rollout.json \
  --installer-unit codifix-ca-tui-installer.service
```
